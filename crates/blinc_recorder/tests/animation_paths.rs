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

#[path = "animation_paths/css_backgrounds.rs"]
mod css_backgrounds;
#[path = "animation_paths/css_groups.rs"]
mod css_groups;

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
    // Animation values are applied on the frames a tick finds one running, as in a window.
    ui.gate_animation_apply();
    ui.load_font_data(
        include_bytes!("../../../examples/blinc_component_demos/assets/NotoSans-Regular.ttf")
            .to_vec(),
    );
    // Geometry and sharpness measurements use the bundled face on every platform.
    ui.css("* { font-family: 'Noto Sans'; }");
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
            cached: bool,
        }
        impl tracing::field::Visit for Fields {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                let text = format!("{value:?}").trim_matches('"').to_string();
                match field.name() {
                    "message" => self.message = text,
                    "path" => self.path = text,
                    "outcome" => self.outcome = text,
                    "used_fast_paint" => self.cached = text == "true",
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
            "p4_breakdown" => self.0.lock().unwrap().push(
                if fields.cached {
                    "cache_repaint"
                } else {
                    "full"
                }
                .into(),
            ),
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

const ICON: &str = r##"<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><circle cx="12" cy="12" r="9" fill="#ffcc33"/><path d="M7 12h10" stroke="#222222" stroke-width="2"/></svg>"##;

/// A scrolled list whose rows each carry an SVG icon, which moves with the rows.
fn scroll_icons_scene(name: &'static str) -> Scene {
    let steps = 20;
    Scene {
        name,
        steps,
        step_ms: 16,
        build: Box::new(move |ui| {
            let reference = blinc_layout::selector::ScrollRef::new();
            let bound = reference.clone();
            ui.mount(move |_| {
                let mut list = div().flex_col().gap(2.0).w(W as f32 - 20.0).p(2.0);
                for i in 0..60 {
                    list = list.child(
                        div()
                            .w(W as f32 - 28.0)
                            .h(28.0)
                            .rounded(6.0)
                            .bg(Color::rgb(0.2, 0.45, 0.7))
                            .flex_row()
                            .items_center()
                            .gap(2.0)
                            .child(blinc_layout::svg::svg(ICON).square(18.0))
                            .child(text(format!("Row {i}")).size(13.0).color(Color::WHITE)),
                    );
                }
                div()
                    .w(W as f32)
                    .h(H as f32)
                    .bg(Color::rgb(0.09, 0.1, 0.13))
                    .child(scroll().bind(&bound).w(W as f32).h(H as f32).child(list))
            });
            Box::new(move |ui, step| {
                reference.set_scroll_offset(0.0, 12.0 * (step + 1) as f32);
                ui.settle_scroll();
            })
        }),
    }
}

/// A scrolled list whose rows are translucent, each a group with a label, which moves with the
/// rows.
fn scroll_translucent_scene(name: &'static str) -> Scene {
    let steps = 20;
    Scene {
        name,
        steps,
        step_ms: 16,
        build: Box::new(move |ui| {
            let reference = blinc_layout::selector::ScrollRef::new();
            let bound = reference.clone();
            ui.mount(move |_| {
                let mut list = div().flex_col().gap(2.0).w(W as f32 - 20.0).p(2.0);
                for i in 0..60 {
                    list = list.child(
                        div()
                            .w(W as f32 - 28.0)
                            .h(28.0)
                            .rounded(6.0)
                            .bg(Color::rgb(0.2, 0.45, 0.7))
                            .border(1.0, Color::rgba(1.0, 1.0, 1.0, 0.8))
                            .opacity(if i % 2 == 0 { 0.6 } else { 1.0 })
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
            Box::new(move |ui, step| {
                reference.set_scroll_offset(0.0, 12.0 * (step + 1) as f32);
                ui.settle_scroll();
            })
        }),
    }
}

/// A scrolled list whose rows each draw a small chart on a canvas, which moves with the rows.
fn scroll_canvas_scene(name: &'static str) -> Scene {
    let steps = 20;
    Scene {
        name,
        steps,
        step_ms: 16,
        build: Box::new(move |ui| {
            let reference = blinc_layout::selector::ScrollRef::new();
            let bound = reference.clone();
            ui.mount(move |_| {
                let mut list = div().flex_col().gap(2.0).w(W as f32 - 20.0).p(2.0);
                for i in 0..60 {
                    let height = 6.0 + (i % 5) as f32 * 3.0;
                    list = list.child(
                        div()
                            .w(W as f32 - 28.0)
                            .h(28.0)
                            .rounded(6.0)
                            .bg(Color::rgb(0.2, 0.45, 0.7))
                            .flex_row()
                            .items_center()
                            .gap(2.0)
                            .child(
                                blinc_layout::canvas::canvas(move |ctx, bounds| {
                                    use blinc_core::DrawContext;
                                    ctx.fill_rect(
                                        blinc_core::Rect::new(
                                            0.0,
                                            bounds.height - height,
                                            bounds.width * 0.5,
                                            height,
                                        ),
                                        2.0.into(),
                                        Color::rgb(0.95, 0.75, 0.2).into(),
                                    );
                                })
                                .w(40.0)
                                .h(22.0),
                            )
                            .child(text(format!("Row {i}")).size(13.0).color(Color::WHITE)),
                    );
                }
                div()
                    .w(W as f32)
                    .h(H as f32)
                    .bg(Color::rgb(0.09, 0.1, 0.13))
                    .child(scroll().bind(&bound).w(W as f32).h(H as f32).child(list))
            });
            Box::new(move |ui, step| {
                reference.set_scroll_offset(0.0, 12.0 * (step + 1) as f32);
                ui.settle_scroll();
            })
        }),
    }
}

/// A scrolled list whose rows have a transform of their own, and one row a card a binding moves,
/// all of which the container's row carries.
fn scroll_transformed_scene(name: &'static str) -> Scene {
    let steps = 20;
    Scene {
        name,
        steps,
        step_ms: 16,
        build: Box::new(move |ui| {
            let reference = blinc_layout::selector::ScrollRef::new();
            let bound_ref = reference.clone();
            let x = bound(ui, 0.0);
            let hx = x.clone();
            ui.mount(move |_| {
                let mut list = div().flex_col().gap(2.0).w(W as f32 - 20.0).p(2.0);
                for i in 0..60 {
                    let mut row = div()
                        .w(W as f32 - 60.0)
                        .h(28.0)
                        .rounded(6.0)
                        .bg(Color::rgb(0.2, 0.45, 0.7))
                        .flex_row()
                        .items_center();
                    // The moving row has no label: a moving label inside a scrolled container is
                    // placed a glyph edge away from a full paint (see the scroll issue).
                    if i != 4 {
                        row = row.child(text(format!("Row {i}")).size(13.0).color(Color::WHITE));
                    }
                    if i % 3 == 0 {
                        row = row.transform(Transform::translate(12.0, 0.0));
                    }
                    if i == 4 {
                        list = list.child(
                            div()
                                .w(W as f32 - 60.0)
                                .h(28.0)
                                .child(row.motion().translate_x(x.clone())),
                        );
                    } else {
                        list = list.child(row);
                    }
                }
                div()
                    .w(W as f32)
                    .h(H as f32)
                    .bg(Color::rgb(0.09, 0.1, 0.13))
                    .child(
                        scroll()
                            .bind(&bound_ref)
                            .w(W as f32)
                            .h(H as f32)
                            .child(list),
                    )
            });
            Box::new(move |ui, step| {
                reference.set_scroll_offset(0.0, 8.0 * (step + 1) as f32);
                hx.lock()
                    .unwrap()
                    .set_target(20.0 * (step + 1) as f32 / steps as f32);
                ui.settle_scroll();
            })
        }),
    }
}

/// A scrolled list with a turning spinner in some rows: the arcs' clip paths follow their own
/// turn and the container's offset.
fn scroll_spinners_scene(name: &'static str) -> Scene {
    Scene {
        name,
        steps: 16,
        step_ms: 16,
        build: Box::new(move |ui| {
            ui.theme(
                blinc_theme::HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
                blinc_theme::ColorScheme::Dark,
            );
            let reference = blinc_layout::selector::ScrollRef::new();
            let bound_ref = reference.clone();
            ui.mount(move |_| {
                let mut list = div().flex_col().gap(2.0).w(W as f32 - 20.0).p(2.0);
                for i in 0..40 {
                    let mut row = div()
                        .w(W as f32 - 60.0)
                        .h(28.0)
                        .rounded(6.0)
                        .bg(Color::rgb(0.2, 0.45, 0.7))
                        .flex_row()
                        .items_center()
                        .gap(2.0)
                        .child(text(format!("Row {i}")).size(13.0).color(Color::WHITE));
                    if i % 2 == 0 {
                        row = row.child(blinc_cn::spinner());
                    }
                    list = list.child(row);
                }
                div()
                    .w(W as f32)
                    .h(H as f32)
                    .bg(Color::rgb(0.09, 0.1, 0.13))
                    .child(
                        scroll()
                            .bind(&bound_ref)
                            .w(W as f32)
                            .h(H as f32)
                            .child(list),
                    )
            });
            Box::new(move |ui, step| {
                reference.set_scroll_offset(0.0, 8.0 * (step + 1) as f32);
                ui.settle_scroll();
            })
        }),
    }
}

/// A scrolled page holding clips of its own: a nested scroll area with icons and lines in it, an image in a
/// rounded clip with an absolute dot over it in a stack, and the same image without the stack.
fn scroll_inner_clips_scene(name: &'static str) -> Scene {
    Scene {
        name,
        steps: 16,
        step_ms: 16,
        build: Box::new(move |ui| {
            let reference = blinc_layout::selector::ScrollRef::new();
            let bound_ref = reference.clone();
            ui.mount(move |_| {
                let picture = || {
                    div().w(36.0).h(36.0).rounded(18.0).overflow_clip().child(
                        blinc_layout::image(PIXEL)
                            .size(36.0, 36.0)
                            .fade_in(std::time::Duration::ZERO),
                    )
                };
                let mut list = div().flex_col().gap(4.0).w(W as f32 - 20.0).p(4.0);
                for i in 0..24 {
                    let mut row = div()
                        .w(W as f32 - 40.0)
                        .flex_row()
                        .gap(4.0)
                        .items_center()
                        .child(picture())
                        .child(
                            blinc_layout::stack::stack()
                                .w(36.0)
                                .h(36.0)
                                .overflow_visible()
                                .child_box(if std::env::var("SV").as_deref() == Ok("dotfirst") {
                                    Box::new(div().w(10.0).h(10.0).bg(Color::rgb(0.2, 0.8, 0.3)))
                                        as Box<dyn ElementBuilder>
                                } else {
                                    Box::new(picture())
                                })
                                .child_box(if std::env::var("SV").as_deref() == Ok("nodot") {
                                    Box::new(div()) as Box<dyn ElementBuilder>
                                } else if std::env::var("SV").as_deref() == Ok("dotfirst") {
                                    Box::new(picture())
                                } else {
                                    Box::new(
                                        div()
                                            .w(10.0)
                                            .h(10.0)
                                            .rounded(5.0)
                                            .bg(Color::rgb(0.2, 0.8, 0.3))
                                            .absolute()
                                            .right(0.0)
                                            .bottom(0.0),
                                    )
                                }),
                        );
                    if i % 3 == 0 {
                        let mut lines = div().flex_col().gap(2.0);
                        for j in 0..8 {
                            lines = lines.child(
                                div()
                                    .flex_row()
                                    .gap(1.0)
                                    .child(blinc_layout::svg(ICON).size(10.0, 10.0))
                                    .child(
                                        text(format!("Line {j}")).size(12.0).color(Color::WHITE),
                                    ),
                            );
                        }
                        row = row.child(
                            scroll()
                                .w(120.0)
                                .h(44.0)
                                .bg(Color::rgb(0.2, 0.25, 0.35))
                                .rounded(6.0)
                                .child(lines),
                        );
                    }
                    list = list.child(row);
                }
                // The viewport sits between a header and a footer, so content that leaves it
                // would show over them.
                div()
                    .w(W as f32)
                    .h(H as f32)
                    .flex_col()
                    .bg(Color::rgb(0.09, 0.1, 0.13))
                    .child(div().w(W as f32).h(40.0).bg(Color::rgb(0.5, 0.2, 0.2)))
                    .child(
                        scroll()
                            .bind(&bound_ref)
                            .w(W as f32)
                            .h(H as f32 - 80.0)
                            .child(list),
                    )
                    .child(div().w(W as f32).h(40.0).bg(Color::rgb(0.5, 0.2, 0.2)))
            });
            Box::new(move |ui, step| {
                // Scrolls for ten steps, then holds still, as scrolling that stops.
                reference.set_scroll_offset(0.0, 6.0 * (step + 1).min(10) as f32);
                ui.settle_scroll();
            })
        }),
    }
}

/// A scrolled page of rounded rows holding an image in a faded group, so the image is drawn in
/// the batch's ordered stream, loaded on the worker as in a window; it scrolls, then stops. Rows
/// decline such a batch today (git-bug 2abee0a).
fn scroll_grouped_images_scene(name: &'static str) -> Scene {
    Scene {
        name,
        steps: 16,
        step_ms: 16,
        build: Box::new(move |ui| {
            // Images load on the worker, as in a window.
            ui.set_async_image_decode(true);
            let reference = blinc_layout::selector::ScrollRef::new();
            let bound_ref = reference.clone();
            ui.mount(move |_| {
                let mut list = div().flex_col().gap(4.0).w(W as f32 - 20.0).p(4.0);
                for i in 0..24 {
                    list = list.child(
                        div()
                            .flex_row()
                            .gap(4.0)
                            .items_center()
                            .rounded(10.0)
                            .overflow_clip()
                            .bg(Color::rgb(0.15, 0.17, 0.22))
                            .p(2.0)
                            .child(
                                div().opacity(0.9).child(
                                    blinc_layout::image(PIXEL)
                                        .size(36.0, 36.0)
                                        .fade_in(std::time::Duration::ZERO),
                                ),
                            )
                            .child(text(format!("Row {i}")).size(13.0).color(Color::WHITE)),
                    );
                }
                div()
                    .w(W as f32)
                    .h(H as f32)
                    .bg(Color::rgb(0.09, 0.1, 0.13))
                    .child(
                        scroll()
                            .bind(&bound_ref)
                            .w(W as f32)
                            .h(H as f32)
                            .child(list),
                    )
            });
            Box::new(move |ui, step| {
                reference.set_scroll_offset(0.0, 6.0 * (step + 1).min(10) as f32);
                ui.settle_scroll();
            })
        }),
    }
}

/// A scrolled list of rounded rows, each with an icon inside a translucent box, as a
/// disabled pagination button is: the group captures the icon as an ordered image.
fn scroll_grouped_icons_scene(name: &'static str) -> Scene {
    Scene {
        name,
        steps: 16,
        step_ms: 16,
        build: Box::new(move |ui| {
            let reference = blinc_layout::selector::ScrollRef::new();
            let bound_ref = reference.clone();
            ui.mount(move |_| {
                let mut list = div().flex_col().gap(4.0).w(W as f32 - 20.0).p(4.0);
                for i in 0..24 {
                    list = list.child(
                        div()
                            .flex_row()
                            .gap(4.0)
                            .items_center()
                            .rounded(10.0)
                            .overflow_clip()
                            .bg(Color::rgb(0.15, 0.17, 0.22))
                            .p(2.0)
                            .child(
                                div()
                                    .opacity(0.5)
                                    .w(28.0)
                                    .h(28.0)
                                    .items_center()
                                    .justify_center()
                                    .border(1.0, Color::rgba(1.0, 1.0, 1.0, 0.4))
                                    .rounded(6.0)
                                    .child(blinc_layout::svg::svg(ICON).square(16.0)),
                            )
                            .child(text(format!("Row {i}")).size(13.0).color(Color::WHITE)),
                    );
                }
                div()
                    .w(W as f32)
                    .h(H as f32)
                    .bg(Color::rgb(0.09, 0.1, 0.13))
                    .child(
                        scroll()
                            .bind(&bound_ref)
                            .w(W as f32)
                            .h(H as f32)
                            .child(list),
                    )
            });
            Box::new(move |ui, step| {
                reference.set_scroll_offset(0.0, 6.0 * (step + 1).min(10) as f32);
                ui.settle_scroll();
            })
        }),
    }
}

/// A scrolled list of clipped rows, each with an icon in a turned box, as an open tree item's
/// chevron is: rows fold the scroll into the icon's transform, so its rect stays where it was
/// painted while its clip moves.
fn scroll_turned_icons_scene(name: &'static str) -> Scene {
    Scene {
        name,
        steps: 16,
        step_ms: 16,
        build: Box::new(move |ui| {
            let reference = blinc_layout::selector::ScrollRef::new();
            let bound_ref = reference.clone();
            ui.mount(move |_| {
                let mut list = div().flex_col().gap(4.0).w(W as f32 - 20.0).p(4.0);
                for i in 0..24 {
                    list = list.child(
                        div()
                            .flex_row()
                            .gap(4.0)
                            .items_center()
                            .rounded(6.0)
                            .overflow_clip()
                            .h(24.0)
                            .bg(Color::rgb(0.15, 0.17, 0.22))
                            .px(2.0)
                            .child(
                                div()
                                    .w(16.0)
                                    .h(16.0)
                                    .transform(Transform::rotate(std::f32::consts::FRAC_PI_2))
                                    .child(blinc_layout::svg::svg(ICON).square(16.0)),
                            )
                            .child(text(format!("Row {i}")).size(13.0).color(Color::WHITE)),
                    );
                }
                div()
                    .w(W as f32)
                    .h(H as f32)
                    .bg(Color::rgb(0.09, 0.1, 0.13))
                    .child(
                        scroll()
                            .bind(&bound_ref)
                            .w(W as f32)
                            .h(H as f32)
                            .child(list),
                    )
            });
            Box::new(move |ui, step| {
                reference.set_scroll_offset(0.0, 6.0 * (step + 1).min(10) as f32);
                ui.settle_scroll();
            })
        }),
    }
}

/// A scrolled list of canvases that fill a path beside a rect, as a chart's pie does.
fn scroll_canvas_paths_scene(name: &'static str) -> Scene {
    Scene {
        name,
        steps: 16,
        step_ms: 16,
        build: Box::new(move |ui| {
            let reference = blinc_layout::selector::ScrollRef::new();
            let bound = reference.clone();
            ui.mount(move |_| {
                let mut list = div().flex_col().gap(2.0).w(W as f32 - 20.0).p(2.0);
                for i in 0..40 {
                    list = list.child(
                        div()
                            .w(W as f32 - 28.0)
                            .h(28.0)
                            .rounded(6.0)
                            .bg(Color::rgb(0.2, 0.45, 0.7))
                            .flex_row()
                            .items_center()
                            .gap(2.0)
                            .child(
                                blinc_layout::canvas::canvas(move |ctx, bounds| {
                                    use blinc_core::DrawContext;
                                    ctx.fill_rect(
                                        blinc_core::Rect::new(0.0, 0.0, 8.0, bounds.height),
                                        2.0.into(),
                                        Color::rgb(0.95, 0.75, 0.2).into(),
                                    );
                                    let path = blinc_core::Path::new()
                                        .move_to(14.0, bounds.height)
                                        .line_to(26.0, 2.0)
                                        .line_to(38.0, bounds.height)
                                        .close();
                                    ctx.fill_path(&path, Color::rgb(0.9, 0.3, 0.5).into());
                                })
                                // Drawn once, as a chart that is not animating is.
                                .redraw_while(|| false)
                                .w(40.0)
                                .h(22.0),
                            )
                            .child(text(format!("Row {i}")).size(13.0).color(Color::WHITE)),
                    );
                }
                div()
                    .w(W as f32)
                    .h(H as f32)
                    .bg(Color::rgb(0.09, 0.1, 0.13))
                    .child(scroll().bind(&bound).w(W as f32).h(H as f32).child(list))
            });
            Box::new(move |ui, step| {
                reference.set_scroll_offset(0.0, 8.0 * (step + 1).min(10) as f32);
                ui.settle_scroll();
            })
        }),
    }
}

/// A card that a binding moves and scales, with an SVG icon in it that moves and grows with it.
fn motion_svg_scene(name: &'static str) -> Scene {
    let steps = 20;
    Scene {
        name,
        steps,
        step_ms: 16,
        build: Box::new(move |ui| {
            let x = bound(ui, 0.0);
            let s = bound(ui, 1.0);
            let (hx, hs) = (x.clone(), s.clone());
            ui.mount(move |_| {
                stage().child(
                    card(false)
                        .child(blinc_layout::svg::svg(ICON).square(28.0))
                        .motion()
                        .translate_x(x.clone())
                        .scale(s.clone()),
                )
            });
            Box::new(move |_, step| {
                let t = (step + 1) as f32 / steps as f32;
                hx.lock().unwrap().set_target(40.0 * t);
                hs.lock().unwrap().set_target(1.0 + 0.3 * t);
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
    transition_css_scene(name, TRANSITION_CSS, label)
}

/// A transition of transform alone, which a binding carries so that rows move it.
const TRANSITION_TRANSFORM_CSS: &str = "
.card { transition: transform 300ms ease-out; }
.card.lift { transform: translateY(-14px) scale(1.15) rotate(8deg); }
";

/// A transition between two poses, neither of them the identity.
const TRANSITION_FROM_A_POSE_CSS: &str = "
.card { transform: translateX(6px) scale(0.9); transition: transform 300ms ease-out; }
.card.lift { transform: translateY(-14px) translateX(-4px) scale(1.15) rotate(8deg); }
";

/// Cards under the CSS in `css`, which lifts them by the class `lift` at the first step.
fn transition_css_scene(name: &'static str, css: &'static str, label: bool) -> Scene {
    Scene {
        name,
        steps: 20,
        step_ms: 16,
        build: Box::new(move |ui| {
            let lift = signal(false);
            ui.css(css);
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

const TRANSITION_FADE_CSS: &str = "
.block { transition: opacity 300ms ease-in-out; }
.block.dim { opacity: 0.25; }
";

/// Blocks of one primitive that a transition of opacity fades without a group.
fn transition_fade_scene(name: &'static str) -> Scene {
    Scene {
        name,
        steps: 20,
        step_ms: 16,
        build: Box::new(move |ui| {
            let dim = signal(false);
            ui.css(TRANSITION_FADE_CSS);
            ui.mount(move |_| {
                (0..3).fold(stage().gap(10.0), |row, _| {
                    row.child(
                        div()
                            .w(70.0)
                            .h(40.0)
                            .rounded(8.0)
                            .bg(Color::rgb(0.2, 0.45, 0.9))
                            .class("block")
                            .class_when("dim", dim),
                    )
                })
            });
            Box::new(move |_, step| {
                if step == 0 {
                    dim.set(true);
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

const KEYFRAME_TRANSFORM_CSS: &str = "
@keyframes bob-turn {
  from { transform: translateY(0px) scale(1) rotate(0deg); }
  to { transform: translateY(-14px) scale(1.15) rotate(8deg); }
}
.card { animation: bob-turn 320ms ease-in-out infinite alternate; }
";

/// Cards under a CSS keyframe animation of transform alone, which a binding carries so that rows
/// move it.
fn keyframe_transform_scene(name: &'static str, label: bool) -> Scene {
    Scene {
        name,
        steps: 20,
        step_ms: 16,
        build: Box::new(move |ui| {
            ui.css(KEYFRAME_TRANSFORM_CSS);
            ui.mount(move |_| {
                (0..2).fold(stage().gap(6.0), |row, _| {
                    row.child(card(label).class("card"))
                })
            });
            Box::new(|_, _| {})
        }),
    }
}

const KEYFRAME_FADE_CSS: &str = "
@keyframes breathe {
  from { opacity: 1; }
  to { opacity: 0.25; }
}
.block { animation: breathe 320ms ease-in-out infinite alternate; }
";

/// A fade in from nothing, which a node painted at no opacity has no ratio to grow from.
const KEYFRAME_FADE_IN_CSS: &str = "
@keyframes appear {
  from { opacity: 0; }
  to { opacity: 1; }
}
.block { animation: appear 320ms ease-out infinite alternate; }
";

/// Blocks under a CSS keyframe animation of opacity alone, each a single filled rectangle (a
/// skeleton placeholder), which fade without a group and so ride rows. `bordered` gives each one a
/// border, which makes it two draws that a fade has to treat as one picture, so it keeps its group.
fn keyframe_fade_scene(name: &'static str, css: &'static str, bordered: bool) -> Scene {
    Scene {
        name,
        steps: 20,
        step_ms: 16,
        build: Box::new(move |ui| {
            ui.css(css);
            ui.mount(move |_| {
                (0..3).fold(stage().gap(10.0), |row, _| {
                    let block = div()
                        .w(70.0)
                        .h(40.0)
                        .rounded(8.0)
                        .bg(Color::rgb(0.2, 0.45, 0.9))
                        .class("block");
                    let block = if bordered {
                        block.border(3.0, Color::rgba(1.0, 1.0, 1.0, 0.9))
                    } else {
                        block
                    };
                    row.child(block)
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

/// Spinners, whose arcs turn under a CSS keyframe rule and carry a clip path that turns with them.
fn spinner_scene(name: &'static str) -> Scene {
    Scene {
        name,
        steps: 12,
        step_ms: 16,
        build: Box::new(|ui| {
            ui.theme(
                blinc_theme::HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
                blinc_theme::ColorScheme::Dark,
            );
            ui.mount(|_| {
                (0..3).fold(stage().gap(6.0), |row, _| {
                    row.child(blinc_cn::spinner().size(blinc_cn::SpinnerSize::Large))
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
        fade_grid_scene_of(6),
        scroll_icons_scene("scroll a list with icons"),
        scroll_translucent_scene("scroll a translucent list"),
        scroll_canvas_scene("scroll a list of canvases"),
        scroll_transformed_scene("scroll a list with transforms and a mover"),
        motion_svg_scene("motion, an svg"),
        spinner_scene("css keyframes, spinners"),
        scroll_spinners_scene("scroll a list of spinners"),
        scroll_inner_clips_scene("scroll a page with clips of its own"),
        scroll_grouped_images_scene("scroll grouped images, then stop"),
        scroll_grouped_icons_scene("scroll grouped icons"),
        scroll_turned_icons_scene("scroll turned icons"),
        scroll_canvas_paths_scene("scroll canvases that fill paths"),
        transition_scene("css transition", true),
        transition_scene("css transition, no text", false),
        transition_css_scene(
            "css transition, transform only, no text",
            TRANSITION_TRANSFORM_CSS,
            false,
        ),
        transition_css_scene(
            "css transition, from a pose, no text",
            TRANSITION_FROM_A_POSE_CSS,
            false,
        ),
        transition_fade_scene("css transition, opacity, one primitive"),
        keyframe_scene("css keyframes", true),
        keyframe_scene("css keyframes, no text", false),
        keyframe_transform_scene("css keyframes, transform only, no text", false),
        keyframe_fade_scene(
            "css keyframes, opacity, one primitive",
            KEYFRAME_FADE_CSS,
            false,
        ),
        keyframe_fade_scene("css keyframes, opacity, bordered", KEYFRAME_FADE_CSS, true),
        keyframe_fade_scene(
            "css keyframes, fade in, one primitive",
            KEYFRAME_FADE_IN_CSS,
            false,
        ),
        flip_scene("flip", true),
        flip_scene("flip, no text", false),
        flip_grid_scene(30),
    ]
    .into_iter()
    // BLINC_ANIM_PATHS_ONLY=<text> keeps the scenes whose name contains it; safe to run with.
    .filter(|s| std::env::var("BLINC_ANIM_PATHS_ONLY").map_or(true, |only| s.name.contains(&only)))
    .collect()
}

/// A pixel differs when a channel is further than this from the reference: edge rounding is not a
/// difference.
const LEVELS: u8 = 2;

/// Scenes in which the patched path is known to differ from a full paint. They are listed so the
/// rest are held to the reference; a scene that stops differing is an error until it is taken off
/// the list. A label stays where the last full paint put it while a bound value moves, an outer
/// binding moving an inner one leaves the inner box and its shadow in the wrong place, and an
/// image or an SVG does not follow the card it is in.
const KNOWN_DIFFERENT: &[&str] = &[
    "motion, an svg",
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
    "css keyframes, transform only, no text",
    "css keyframes, opacity, one primitive",
    "motion fade, equal cards",
    "scroll a list with icons",
    "scroll a translucent list",
    "scroll a list of canvases",
    "scroll a list with transforms and a mover",
    "motion, an svg",
    "css keyframes, spinners",
    "scroll a list of spinners",
    "scroll grouped images, then stop",
    "scroll grouped icons",
    "scroll turned icons",
    "scroll a page with clips of its own",
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

/// A card scaled to 2x by a CSS keyframe animation over `ms`, with a fade too when `fade`. Held
/// there when it ends, or, with `repeat`, started over for ever: the harness settles for ten
/// seconds before the first frame, so a repeating animation of 1600 ms is a quarter through it
/// and the frames that follow are in flight.
fn css_zoom_scene(label: bool, fade: bool, ms: u32, repeat: bool) -> Scene {
    Scene {
        name: "css zoom",
        steps: 40,
        step_ms: 16,
        build: Box::new(move |ui| {
            let play = if repeat { "infinite" } else { "forwards" };
            let css = if fade {
                format!(
                    "@keyframes zoom {{ from {{ transform: scale(1); opacity: 1; }} \
                     to {{ transform: scale(2); opacity: 0.9; }} }} \
                     .zoom {{ animation: zoom {ms}ms linear {play}; }}"
                )
            } else {
                format!(
                    "@keyframes zoom {{ from {{ transform: scale(1); }} to {{ transform: scale(2); }} }} \
                     .zoom {{ animation: zoom {ms}ms linear {play}; }}"
                )
            };
            ui.css(&css);
            ui.mount(move |_| {
                stage().child(
                    div()
                        .w(120.0)
                        .h(64.0)
                        .child(card_at(label, 1.0).class("zoom")),
                )
            });
            Box::new(|_, _| {})
        }),
    }
}

#[test]
fn text_zoomed_by_a_css_animation_is_as_sharp_as_text_laid_out_at_that_size() {
    let mut report = Vec::new();
    for fade in [false, true] {
        for path in [Path::Vector, Path::Table, Path::Rows] {
            let (Some(with), Some(without)) = (
                run(&css_zoom_scene(true, fade, 300, false), path),
                run(&css_zoom_scene(false, fade, 300, false), path),
            ) else {
                return;
            };
            let (Some(native_with), Some(native_without)) = (
                run(&native_scene(2.0, true), Path::Vector),
                run(&native_scene(2.0, false), Path::Vector),
            ) else {
                return;
            };
            let zoomed = sharpness(with.last().unwrap(), without.last().unwrap());
            let native = sharpness(&native_with[0], &native_without[0]);
            eprintln!(
                "CSS SHARP fade {fade} {path:?}: zoomed {zoomed:.3}, native {native:.3}, ratio {:.2}",
                zoomed / native
            );
            report.push((fade, path, zoomed / native));
        }
    }
    for (fade, path, ratio) in report {
        assert!(
            ratio > 0.95,
            "fade {fade}, {path:?}: css-zoomed text is {ratio:.2} as sharp as text laid out at that size"
        );
    }
}

/// Width of the ink a label adds, from the frames with and without it.
fn ink_width(with: &CapturedFrame, without: &CapturedFrame) -> f64 {
    let (w, h) = (with.width as usize, with.height as usize);
    let (mut lo, mut hi) = (usize::MAX, 0);
    for y in 0..h {
        for x in 0..w {
            let (p, q) = (pixel(with, x, y), pixel(without, x, y));
            if (0..3).map(|i| (p[i] - q[i]).abs()).sum::<i32>() > 60 {
                lo = lo.min(x);
                hi = hi.max(x);
            }
        }
    }
    if hi >= lo { (hi - lo + 1) as f64 } else { 0.0 }
}

/// How sharp the label of a card at the middle of the stage is, from one frame: the energy of the
/// gradient over the energy of the pixels, taken against the card's colour, in a window around the
/// label that stays inside the card at scale `s`.
fn label_sharpness(frame: &CapturedFrame, s: f32) -> f64 {
    let (cx, cy) = (frame.width as f32 / 2.0, frame.height as f32 / 2.0);
    let (x0, x1) = ((cx - 22.0 * s) as usize, (cx + 22.0 * s) as usize);
    let (y0, y1) = ((cy - 12.0 * s) as usize, (cy + 12.0 * s) as usize);
    let bg = pixel(frame, x0, y0);
    let ink = |x: usize, y: usize| -> f64 {
        let p = pixel(frame, x, y);
        (0..3).map(|i| (p[i] - bg[i]).abs() as f64).sum::<f64>()
    };
    let (mut grad, mut energy) = (0.0, 0.0);
    for y in y0 + 1..y1 {
        for x in x0 + 1..x1 {
            let v = ink(x, y);
            let (dx, dy) = (ink(x + 1, y) - ink(x - 1, y), ink(x, y + 1) - ink(x, y - 1));
            grad += dx * dx + dy * dy;
            energy += v * v;
        }
    }
    grad / energy.max(1.0)
}

/// How sharp the label of a card that a CSS animation scales is in flight, against the same label
/// laid out natively at the scale the card has in that frame (read from the width of its ink in the
/// run without a fade). The animation repeats, so the frames are in the middle of it.
fn in_flight_ratios(fade: bool, path: Path, steps: &[usize]) -> Option<Vec<f64>> {
    let w1 = ink_width(
        &run(&native_scene(1.0, true), Path::Vector)?[0],
        &run(&native_scene(1.0, false), Path::Vector)?[0],
    );
    let plain = (
        run(&css_zoom_scene(true, false, 1600, true), path)?,
        run(&css_zoom_scene(false, false, 1600, true), path)?,
    );
    let shown = if fade {
        run(&css_zoom_scene(true, true, 1600, true), path)?
    } else {
        plain.0.clone()
    };
    steps
        .iter()
        .map(|&step| {
            let s = (ink_width(&plain.0[step], &plain.1[step]) / w1) as f32;
            let native = label_sharpness(&run(&native_scene(s, true), Path::Vector)?[0], s);
            Some(label_sharpness(&shown[step], s) / native)
        })
        .collect()
}

#[test]
fn text_zoomed_by_a_spring_is_sharp_while_it_moves() {
    let w1 = {
        let (Some(a), Some(b)) = (
            run(&native_scene(1.0, true), Path::Vector),
            run(&native_scene(1.0, false), Path::Vector),
        ) else {
            return;
        };
        ink_width(&a[0], &b[0])
    };
    for path in [Path::Vector, Path::Rows, Path::Table] {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (Some(with), Some(without)) = (
            run(&zoom_scene(2.0, true, seen.clone()), path),
            run(&zoom_scene(2.0, false, seen), path),
        ) else {
            return;
        };
        let mut ratios = Vec::new();
        for step in [4_usize, 7, 10, 13] {
            let s = (ink_width(&with[step], &without[step]) / w1) as f32;
            let Some(native) = run(&native_scene(s, true), Path::Vector) else {
                return;
            };
            ratios.push(label_sharpness(&with[step], s) / label_sharpness(&native[0], s));
        }
        eprintln!("SPRING IN FLIGHT {path:?}: {ratios:.2?}");
        for ratio in ratios {
            // The scale is read from the width of the ink to a pixel, which is a few percent of
            // it early in the zoom, when the scale changes most between frames.
            assert!(
                ratio > 0.9,
                "{path:?}: a label under a zooming spring is {ratio:.2} as sharp as text laid out \
                 at the scale it has"
            );
        }
    }
}

#[test]
fn text_scaled_by_a_css_animation_is_sharp_while_it_plays() {
    for (fade, path) in [
        (false, Path::Vector),
        (false, Path::Rows),
        (false, Path::Table),
        (true, Path::Rows),
    ] {
        let Some(ratios) = in_flight_ratios(fade, path, &[2, 12, 22, 32]) else {
            return;
        };
        eprintln!("IN FLIGHT fade {fade} {path:?}: {ratios:.2?}");
        // The glyph atlas keeps one bitmap for each half pixel of size, so the raster size is
        // on that step and a bitmap is a little off 1:1 with the scale between steps. The
        // label was 0.81 to 0.91 as sharp before it followed the scale and was put on the
        // pixel grid.
        for ratio in ratios {
            assert!(
                ratio > 0.9,
                "fade {fade}, {path:?}: a label under a scale animation is {ratio:.2} as sharp as \
                 text laid out at the scale it has"
            );
        }
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

/// A grid of `count` small bordered cards with a label, each fading by a value of its own: every
/// one paints more than one draw, so each fade is a group.
fn fade_grid_scene(count: usize) -> Scene {
    fade_grid(count, true)
}

fn fade_grid_scene_of(count: usize) -> Scene {
    fade_grid(count, true)
}

fn fade_grid(count: usize, label: bool) -> Scene {
    let steps = if count > 20 { 60 } else { 24 };
    Scene {
        name: "motion fade, equal cards",
        steps,
        step_ms: 16,
        build: Box::new(move |ui| {
            let values: Vec<_> = (0..count).map(|_| bound(ui, 1.0)).collect();
            let handles = values.clone();
            ui.mount(move |_| {
                let mut grid = div()
                    .w(W as f32)
                    .h(H as f32)
                    .bg(Color::rgb(0.09, 0.1, 0.13))
                    .flex_row()
                    .flex_wrap();
                for o in &values {
                    let card = div()
                        .w(26.0)
                        .h(14.0)
                        .rounded(3.0)
                        .bg(Color::rgb(0.2, 0.45, 0.9))
                        .border(1.0, Color::rgba(1.0, 1.0, 1.0, 0.9))
                        .flex_row()
                        .items_center()
                        .justify_center();
                    let card = if label {
                        card.child(text("ab").size(8.0).color(Color::WHITE))
                    } else {
                        card
                    };
                    grid = grid.child(
                        div()
                            .w(30.0)
                            .h(16.0)
                            .child(card.motion().opacity(o.clone())),
                    );
                }
                grid
            });
            Box::new(move |_, step| {
                let t = (step + 1) as f32 / steps as f32;
                for o in &handles {
                    o.lock().unwrap().set_target(1.0 - 0.6 * t);
                }
            })
        }),
    }
}

/// Not a check: what a frame costs with many cards each fading as a group. Run with
/// `--run-ignored all --no-capture`.
#[test]
#[ignore]
fn what_a_frame_costs_with_many_faded_cards() {
    for (label, count) in [
        (true, 10),
        (true, 40),
        (true, 140),
        (false, 40),
        (false, 140),
    ] {
        let scene = fade_grid(count, label);
        eprintln!(
            "{count} cards, {}, {} frames each (median / p90 ms):",
            if label { "with text" } else { "no text" },
            scene.steps
        );
        for (name, path) in [
            ("promoted, full paint", Path::Full),
            ("vectors, full paint", Path::Vector),
            ("rows on the CPU", Path::Rows),
            ("node table", Path::Table),
        ] {
            let Some((_, paths)) = run_logged(&scene, path) else {
                return;
            };
            let in_full = paths
                .iter()
                .skip(3)
                .filter(|p| p.starts_with("full"))
                .count();
            let Some(mut times) = frame_times(&scene, path) else {
                return;
            };
            times.drain(..3);
            times.sort_by(|a, b| a.total_cmp(b));
            eprintln!(
                "  {name:<22} {:>7.2} / {:>7.2}   {in_full:>2} of {} in full",
                times[times.len() / 2],
                times[times.len() * 9 / 10],
                paths.len() - 3
            );
        }
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

/// When a FLIP slide ends its binding is taken away and the card is drawn where layout put it, as
/// a full paint has it. The window applies animation values only on a frame in which something
/// was running, and the frame a slide ends in is one of those.
#[test]
fn a_flip_slide_leaves_nothing_bound_when_it_ends() {
    let mut scene = flip_scene("flip, no text", false);
    // The slide is about 19 frames; the rest are frames at rest.
    scene.steps += 6;
    let Some(reference) = run(&scene, Path::Vector) else {
        return;
    };
    let Some(mut ui) = session(Path::Rows) else {
        return;
    };
    let mut script = (scene.build)(&mut ui);
    ui.settle();
    ui.snapshot().expect("a frame");
    let mut last = None;
    for step in 0..scene.steps {
        script(&mut ui, step);
        ui.advance(scene.step_ms);
        last = Some(ui.snapshot_fast().expect("a frame"));
    }
    assert!(
        ui.tree().motion_bindings_map().is_empty(),
        "the slide ended and its binding is still on the card"
    );
    let d = difference(&last.unwrap(), reference.last().unwrap(), LEVELS);
    assert_eq!(
        d.differing, 0,
        "the card is not where a full paint puts it at rest"
    );
}

/// A CSS animation of transform that rows carry moves the label with its card, to within what the
/// glyphs allow: the label is the card's centre and turns with it, and the picture is the full
/// paint's in all but the glyph edges. Every frame after the first is moved by rows.
#[test]
fn a_css_animated_label_follows_its_card_on_rows() {
    let with = keyframe_transform_scene("css keyframes, transform only", true);
    let without = keyframe_transform_scene("css keyframes, transform only, no text", false);
    let (Some((rows, paths)), Some(bare), Some(reference)) = (
        run_logged(&with, Path::Rows),
        run(&without, Path::Rows),
        run(&with, Path::Vector),
    ) else {
        return;
    };
    let by_rows = paths.iter().filter(|p| p.ends_with("rows:applied")).count();
    assert!(
        by_rows >= with.steps - 3,
        "{by_rows} of {} frames moved by rows: {paths:?}",
        with.steps
    );
    for step in 0..with.steps {
        let card = card_blob(&bare[step]);
        let label = label_blob(&rows[step], &bare[step]);
        assert!(label.pixels > 30, "step {step}: the label left no pixels");
        let off = ((label.centre.0 - card.centre.0).powi(2)
            + (label.centre.1 - card.centre.1).powi(2))
        .sqrt();
        let d = difference(&rows[step], &reference[step], 8);
        // The ink of the label is the full paint's: a label that is smeared has less of it.
        let ours = ink(&rows[step], &bare[step]);
        let theirs = ink(&reference[step], &bare[step]);
        eprintln!(
            "step {step:>2}: label {off:>3.1} px from the card's centre, {} px differ from the full paint, ink {:.2} of it",
            d.differing,
            ours / theirs
        );
        assert!(
            off <= 3.0,
            "step {step}: the label is {off} px off its card"
        );
        assert!(
            d.differing < 1200,
            "step {step}: {} px differ from the full paint",
            d.differing
        );
        assert!(
            ours >= theirs * 0.9,
            "step {step}: the label has {:.0}% of the ink of the full paint",
            100.0 * ours / theirs
        );
    }
}

/// A CSS transition that a binding carries is moved by rows between its first frame, which paints
/// it, and its last, which writes its final values: the frames in the middle only write rows.
#[test]
fn a_css_transition_of_transform_or_opacity_rides_rows() {
    for scene in [
        transition_css_scene(
            "css transition, transform only, no text",
            TRANSITION_TRANSFORM_CSS,
            false,
        ),
        transition_css_scene(
            "css transition, from a pose, no text",
            TRANSITION_FROM_A_POSE_CSS,
            false,
        ),
        transition_fade_scene("css transition, opacity, one primitive"),
    ] {
        let Some((_, paths)) = run_logged(&scene, Path::Rows) else {
            return;
        };
        let by_rows = paths.iter().filter(|p| p.ends_with("rows:applied")).count();
        assert!(
            by_rows >= scene.steps - 4,
            "{}: {by_rows} of {} frames moved by rows: {paths:?}",
            scene.name,
            scene.steps
        );
    }
}

/// The label of a card that a transition moves follows it on rows, as the label of a keyframe
/// animation does: the picture is the full paint's in all but the glyph edges.
#[test]
fn a_css_transitioned_label_follows_its_card_on_rows() {
    let with = transition_css_scene(
        "css transition, transform only",
        TRANSITION_FROM_A_POSE_CSS,
        true,
    );
    let without = transition_css_scene(
        "css transition, from a pose, no text",
        TRANSITION_FROM_A_POSE_CSS,
        false,
    );
    let (Some((rows, paths)), Some(bare), Some(reference)) = (
        run_logged(&with, Path::Rows),
        run(&without, Path::Rows),
        run(&with, Path::Vector),
    ) else {
        return;
    };
    let by_rows = paths.iter().filter(|p| p.ends_with("rows:applied")).count();
    assert!(
        by_rows >= with.steps - 4,
        "{by_rows} of {} frames moved by rows: {paths:?}",
        with.steps
    );
    for step in 0..with.steps {
        let card = card_blob(&bare[step]);
        let label = label_blob(&rows[step], &bare[step]);
        assert!(label.pixels > 30, "step {step}: the label left no pixels");
        let off = ((label.centre.0 - card.centre.0).powi(2)
            + (label.centre.1 - card.centre.1).powi(2))
        .sqrt();
        let d = difference(&rows[step], &reference[step], 8);
        let ours = ink(&rows[step], &bare[step]);
        let theirs = ink(&reference[step], &bare[step]);
        assert!(
            off <= 3.0,
            "step {step}: the label is {off} px off its card"
        );
        assert!(
            d.differing < 1200,
            "step {step}: {} px differ from the full paint",
            d.differing
        );
        assert!(
            ours >= theirs * 0.9,
            "step {step}: the label has {:.0}% of the ink of the full paint",
            100.0 * ours / theirs
        );
    }
}

/// A CSS animation of opacity on a block of one primitive is moved by rows and shows: the block's
/// centre is the stage's at the faint end and the block's at the opaque end.
#[test]
fn a_css_fade_of_a_single_primitive_rides_rows_and_shows() {
    let scene = keyframe_fade_scene(
        "css keyframes, opacity, one primitive",
        KEYFRAME_FADE_CSS,
        false,
    );
    let Some((frames, paths)) = run_logged(&scene, Path::Rows) else {
        return;
    };
    let by_rows = paths.iter().filter(|p| p.ends_with("rows:applied")).count();
    assert_eq!(by_rows, scene.steps, "{paths:?}");
    let blue = |f: &CapturedFrame| pixel(f, (W / 2) as usize, (H / 2) as usize)[2];
    let (lo, hi) = frames
        .iter()
        .map(blue)
        .fold((255, 0), |(lo, hi): (i32, i32), b| (lo.min(b), hi.max(b)));
    assert!(
        hi - lo > 80,
        "the block's centre only moved from blue {lo} to {hi}"
    );
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

// ---- cn components whose open and close are visual or layout animations ----------------------

/// What the cn disclosure components stand on: a column on the scene's ground.
fn cn_stage() -> Div {
    div()
        .w(W as f32)
        .h(H as f32)
        .bg(Color::rgb(0.09, 0.1, 0.13))
        .flex_col()
        .gap(8.0)
        .p(8.0)
}

/// Lines of text, the content of a section that opens.
fn lines(count: usize) -> Div {
    (0..count).fold(div().w_full().p(4.0).flex_col().gap(4.0), |d, i| {
        d.child(text(format!("Line {i}")).size(12.0).color(Color::WHITE))
    })
}

const CN_ICON: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="8"/></svg>"#;

/// A cn disclosure scene: what is mounted, and what happens at step 0.
fn cn_scene(
    name: &'static str,
    steps: usize,
    mount: impl Fn(&mut Headless) -> Box<dyn Fn(&mut Headless)> + 'static,
) -> Scene {
    Scene {
        name,
        steps,
        step_ms: 16,
        build: Box::new(move |ui| {
            let start = mount(ui);
            Box::new(move |ui, step| {
                if step == 0 {
                    start(ui);
                }
            })
        }),
    }
}

/// A collapsible section opening above a card.
fn cn_collapsible_scene() -> Scene {
    cn_scene("cn collapsible section", 30, |ui| {
        let open = signal(false);
        ui.mount(move |_| {
            cn_stage()
                .child(blinc_cn::cn::collapsible_section("Details", open, || {
                    lines(5)
                }))
                .child(card(true))
        });
        Box::new(move |_| open.set(true))
    })
}

/// An accordion opening one section, with a second section and a card below it.
fn cn_accordion_scene() -> Scene {
    cn_scene("cn accordion", 30, |ui| {
        let first = signal(false);
        let second = signal(false);
        ui.mount(move |_| {
            cn_stage()
                .child(
                    blinc_cn::cn::accordion()
                        .item_with_state("a", "First", first, || lines(4))
                        .item_with_state("b", "Second", second, || lines(3)),
                )
                .child(card(true))
        });
        Box::new(move |_| first.set(true))
    })
}

/// A tree expanding its first group over a second.
fn cn_tree_scene() -> Scene {
    cn_scene("cn tree", 30, |ui| {
        ui.mount(move |_| {
            cn_stage().child(
                blinc_cn::cn::tree_view()
                    .node("src", "src/", |n| {
                        n.child("main", "main.rs", |n| n)
                            .child("lib", "lib.rs", |n| n)
                            .child("util", "util.rs", |n| n)
                            .child("mod", "mod.rs", |n| n)
                    })
                    .node("docs", "docs/", |n| n.child("readme", "README.md", |n| n))
                    .node("cargo", "Cargo.toml", |n| n),
            )
        });
        Box::new(|ui| {
            ui.click(".cn-tree-row");
        })
    })
}

/// A sidebar collapsing to its icons beside a card.
fn cn_sidebar_scene() -> Scene {
    cn_scene("cn sidebar", 30, |ui| {
        let collapsed = signal(false);
        ui.mount(move |_| {
            div()
                .w(W as f32)
                .h(H as f32)
                .bg(Color::rgb(0.09, 0.1, 0.13))
                .flex_row()
                .child(
                    blinc_cn::cn::sidebar(collapsed)
                        .item_active("Dashboard", CN_ICON, || {})
                        .item("Search", CN_ICON, || {})
                        .item("Inbox", CN_ICON, || {})
                        .section("Account")
                        .item("Profile", CN_ICON, || {})
                        .content(|_| div().p(16.0).child(card(true))),
                )
        });
        Box::new(move |_| collapsed.set(true))
    })
}

/// Tabs switching: the indicator slides from one trigger to the next.
fn cn_tabs_scene() -> Scene {
    cn_scene("cn tabs", 30, |ui| {
        let state = signal("account".to_string());
        ui.mount(move |_| {
            cn_stage().child(
                blinc_cn::cn::tabs(state.clone())
                    .tab("account", "Account", || lines(2))
                    .tab("password", "Password", || lines(3))
                    .tab("billing", "Billing", || lines(2)),
            )
        });
        Box::new(move |_| state.set("billing".to_string()))
    })
}

/// cn skeletons and an indeterminate progress bar: childless filled blocks under a CSS fade.
fn cn_skeleton_scene() -> Scene {
    cn_scene("cn skeletons", 40, |ui| {
        ui.mount(move |_| {
            cn_stage()
                .child(blinc_cn::cn::skeleton().w(200.0).h(16.0))
                .child(blinc_cn::cn::skeleton().w(140.0).h(16.0))
                .child(blinc_cn::cn::skeleton_circle(40.0))
                .child(blinc_cn::cn::progress_indeterminate().w(180.0))
        });
        Box::new(|_| {})
    })
}

/// cn skeletons fade on rows, as a full paint draws them, and the fade shows.
#[test]
fn cn_skeletons_fade_on_rows() {
    let scene = cn_skeleton_scene();
    let Some((report, _, painted, paths)) = compare(&scene, Path::Rows) else {
        return;
    };
    let by_rows = paths.iter().filter(|p| p.ends_with("rows:applied")).count();
    assert!(
        by_rows >= scene.steps - 3,
        "{by_rows} of {} frames on rows: {paths:?}",
        scene.steps
    );
    assert_eq!(report.differing_steps(), 0, "{:?}", report.per_step);
    let moved = painted
        .windows(2)
        .filter(|w| w[0].data != w[1].data)
        .count();
    assert!(moved >= 20, "the picture changed in only {moved} frames");
}

fn cn_scenes() -> Vec<Scene> {
    vec![
        cn_collapsible_scene(),
        cn_accordion_scene(),
        cn_tree_scene(),
        cn_sidebar_scene(),
        cn_tabs_scene(),
    ]
}

/// Each cn scene really animates: the picture changes over many of its frames, and then stops.
/// Without this a scene that never opens would measure a static frame and say it is cheap.
#[test]
fn the_cn_disclosure_scenes_animate() {
    for scene in cn_scenes() {
        let Some((frames, _)) = run_logged(&scene, Path::Full) else {
            return;
        };
        let moved = frames.windows(2).filter(|w| w[0].data != w[1].data).count();
        let changed = difference(&frames[0], frames.last().unwrap(), LEVELS).differing;
        assert!(
            moved >= 6 && changed > 200,
            "{}: the picture changed in {moved} frames and {changed} px between the first and last",
            scene.name
        );
    }
}

/// Not a check: how each cn disclosure scene is painted and what its moving frames cost, painted
/// in full and as a window paints a frame with only animated values changed. Run with
/// `--run-ignored all --no-capture`; `BLINC_ANIM_PATHS_SCALE=2` runs it at a retina density.
#[test]
#[ignore]
fn how_the_cn_disclosures_are_painted() {
    for scene in cn_scenes() {
        eprintln!("{} ({} frames):", scene.name, scene.steps);
        for (name, path) in [
            ("full paint", Path::Full),
            ("promoted, patched", Path::Patched),
            ("rows on the CPU", Path::Rows),
        ] {
            let Some((frames, paths)) = run_logged(&scene, path) else {
                return;
            };
            // The frames in which the picture moves, the first included: what is measured is
            // these, since the frames after the animation ends are cached whatever the path.
            let moving: Vec<bool> = (0..frames.len())
                .map(|i| i == 0 || frames[i].data != frames[i - 1].data)
                .collect();
            let in_full = (0..paths.len())
                .filter(|&i| moving[i] && paths[i].starts_with("full"))
                .count();
            let animated = moving.iter().filter(|m| **m).count();
            let Some(times) = frame_times(&scene, path) else {
                return;
            };
            let mut cost: Vec<f64> = times
                .iter()
                .enumerate()
                .skip(2)
                .filter(|(i, _)| moving[*i])
                .map(|(_, t)| *t)
                .collect();
            cost.sort_by(|a, b| a.total_cmp(b));
            eprintln!(
                "  {name:<18} {in_full:>2} of {animated:>2} moving frames in full, {:>6.2} / {:>6.2} ms (median / p90)",
                cost[cost.len() / 2],
                cost[cost.len() * 9 / 10]
            );
        }
    }
}

// ---- many cards under a CSS animation ---------------------------------------------------------

/// A CSS animation of transform alone, which a binding carries so that rows move it.
const CSS_GRID_TRANSFORM: &str = "
@keyframes pulse {
  from { transform: translateY(0px) scale(1); }
  to { transform: translateY(-3px) scale(1.12); }
}
.pulse { animation: pulse 400ms ease-in-out infinite alternate; }
";

/// A group animated by transform and opacity.
const CSS_GRID_FADE: &str = "
@keyframes pulse {
  from { transform: translateY(0px) scale(1); opacity: 1; }
  to { transform: translateY(-3px) scale(1.12); opacity: 0.6; }
}
.pulse { animation: pulse 400ms ease-in-out infinite alternate; }
";

/// Opacity alone, applied to a primitive or to a bordered card as a group.
const CSS_GRID_OPACITY: &str = "
@keyframes pulse {
  from { opacity: 1; }
  to { opacity: 0.35; }
}
.pulse { animation: pulse 400ms ease-in-out infinite alternate; }
";

/// A grid of `count` small cards, each under a CSS keyframe animation of transform and opacity,
/// with a label if asked.
fn css_grid_scene(count: usize, label: bool, css: &'static str) -> Scene {
    css_grid(count, label, true, css)
}

/// A grid of `count` plain filled blocks, each under a CSS keyframe animation: a skeleton page.
fn css_blocks_scene(count: usize, css: &'static str) -> Scene {
    css_grid(count, false, false, css)
}

fn css_grid(count: usize, label: bool, bordered: bool, css: &'static str) -> Scene {
    Scene {
        name: "css grid",
        steps: 40,
        step_ms: 16,
        build: Box::new(move |ui| {
            ui.css(css);
            ui.mount(move |_| {
                let mut grid = div()
                    .w(W as f32)
                    .h(H as f32)
                    .bg(Color::rgb(0.09, 0.1, 0.13))
                    .flex_row()
                    .flex_wrap();
                for i in 0..count {
                    let mut card = div()
                        .class("pulse")
                        .w(26.0)
                        .h(14.0)
                        .rounded(3.0)
                        .bg(Color::rgb(0.2, 0.45, 0.9))
                        .flex_row()
                        .items_center()
                        .justify_center();
                    if bordered {
                        card = card.border(1.0, Color::rgba(1.0, 1.0, 1.0, 0.9));
                    }
                    if label {
                        card =
                            card.child(text(format!("{}", i % 10)).size(8.0).color(Color::WHITE));
                    }
                    grid = grid.child(div().w(30.0).h(16.0).child(card));
                }
                grid
            });
            Box::new(|_, _| {})
        }),
    }
}

const CSS_GRID_TRANSITION: &str = "
.pulse { transition: transform 600ms ease-in-out; }
.pulse.lift { transform: translateY(-3px) scale(1.12); }
";

/// A grid of `count` cards that a transition of transform lifts at the first step.
fn css_transition_grid(count: usize, label: bool) -> Scene {
    Scene {
        name: "css transition grid",
        steps: 40,
        step_ms: 16,
        build: Box::new(move |ui| {
            let lift = signal(false);
            ui.css(CSS_GRID_TRANSITION);
            ui.mount(move |_| {
                let mut grid = div()
                    .w(W as f32)
                    .h(H as f32)
                    .bg(Color::rgb(0.09, 0.1, 0.13))
                    .flex_row()
                    .flex_wrap();
                for i in 0..count {
                    let mut card = div()
                        .class("pulse")
                        .class_when("lift", lift)
                        .w(26.0)
                        .h(14.0)
                        .rounded(3.0)
                        .bg(Color::rgb(0.2, 0.45, 0.9))
                        .border(1.0, Color::rgba(1.0, 1.0, 1.0, 0.9))
                        .flex_row()
                        .items_center()
                        .justify_center();
                    if label {
                        card =
                            card.child(text(format!("{}", i % 10)).size(8.0).color(Color::WHITE));
                    }
                    grid = grid.child(div().w(30.0).h(16.0).child(card));
                }
                grid
            });
            Box::new(move |_, step| {
                if step == 0 {
                    lift.set(true);
                }
            })
        }),
    }
}

/// Not a check: what a frame costs with many cards under a CSS transition of transform. Run with
/// `--run-ignored all --no-capture`.
#[test]
#[ignore]
fn what_a_css_transition_frame_costs() {
    for label in [true, false] {
        for count in [20, 60, 140] {
            let scene = css_transition_grid(count, label);
            eprintln!(
                "{count} cards, {}, {} frames each (median / p90 ms, frames in full):",
                if label { "with text" } else { "no text" },
                scene.steps
            );
            for (name, path) in [
                ("full paint", Path::Full),
                ("baked, patched", Path::Patched),
                ("rows on the CPU", Path::Rows),
            ] {
                let Some((_, paths)) = run_logged(&scene, path) else {
                    return;
                };
                let in_full = paths
                    .iter()
                    .skip(3)
                    .filter(|p| p.starts_with("full"))
                    .count();
                let Some(mut times) = frame_times(&scene, path) else {
                    return;
                };
                times.drain(..3);
                times.sort_by(|a, b| a.total_cmp(b));
                eprintln!(
                    "  {name:<18} {:>7.2} / {:>7.2}   {in_full:>2} of {} in full",
                    times[times.len() / 2],
                    times[times.len() * 9 / 10],
                    paths.len() - 3
                );
            }
        }
    }
}

/// Not a check: what a frame costs with many cards under a CSS animation, painted in full, as a
/// window paints it with its bound nodes baked, and moved by rows. Run with
/// `--run-ignored all --no-capture`.
#[test]
#[ignore]
fn what_a_css_animated_frame_costs() {
    for (kind, css, blocks) in [
        ("transform", CSS_GRID_TRANSFORM, false),
        ("transform and opacity", CSS_GRID_FADE, false),
        ("opacity, plain blocks", CSS_GRID_OPACITY, true),
    ] {
        for label in [true, false] {
            if blocks && label {
                continue;
            }
            for count in [20, 60, 140] {
                let scene = if blocks {
                    css_blocks_scene(count, css)
                } else {
                    css_grid_scene(count, label, css)
                };
                eprintln!(
                    "{count} cards, {kind}, {}, {} frames each (median / p90 ms, frames in full):",
                    if label { "with text" } else { "no text" },
                    scene.steps
                );
                for (name, path) in [
                    ("full paint", Path::Full),
                    ("baked, patched", Path::Patched),
                    ("rows on the CPU", Path::Rows),
                ] {
                    let Some((_, paths)) = run_logged(&scene, path) else {
                        return;
                    };
                    let in_full = paths
                        .iter()
                        .skip(3)
                        .filter(|p| p.starts_with("full"))
                        .count();
                    let Some(mut times) = frame_times(&scene, path) else {
                        return;
                    };
                    times.drain(..3);
                    times.sort_by(|a, b| a.total_cmp(b));
                    eprintln!(
                        "  {name:<18} {:>7.2} / {:>7.2}   {in_full:>2} of {} in full",
                        times[times.len() / 2],
                        times[times.len() * 9 / 10],
                        paths.len() - 3
                    );
                }
            }
        }
    }
}

fn resident_kb() -> u64 {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .expect("ps");
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .unwrap_or(0)
}

/// Not a check: how far the process grows while 140 cards run a CSS animation, with the cards
/// baked into textures (`MEM_PATH=patched`, the default) or moved by rows (`MEM_PATH=rows`), or with
/// `MEM_SCENE=blocks` 140 plain blocks fading. Run each in a process of its own, with `--run-ignored all --no-capture`.
#[test]
#[ignore]
fn how_much_memory_css_animated_cards_hold() {
    let path = if std::env::var("MEM_PATH").as_deref() == Ok("rows") {
        Path::Rows
    } else {
        Path::Patched
    };
    let scene = if std::env::var("MEM_SCENE").as_deref() == Ok("blocks") {
        css_blocks_scene(140, CSS_GRID_OPACITY)
    } else {
        css_grid_scene(140, false, CSS_GRID_TRANSFORM)
    };
    let Some(mut ui) = session(path) else { return };
    let mut script = (scene.build)(&mut ui);
    ui.settle();
    ui.snapshot().expect("a frame");
    let before = resident_kb();
    let mut peak = before;
    for step in 0..60 {
        script(&mut ui, step);
        ui.advance(scene.step_ms);
        ui.snapshot_fast().expect("a frame");
        peak = peak.max(resident_kb());
    }
    eprintln!(
        "MEM {path:?}: before {} MB, peak {} MB, grew {} MB",
        before / 1024,
        peak / 1024,
        (peak - before) / 1024
    );
}
