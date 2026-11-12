#![allow(deprecated, dead_code)]
//! Motion-bound nodes in a window, for looking at the GPU node table.
//!
//! Nine small scenes, each driven by springs (`set_target`, as an app does), flip between two poses
//! every couple of seconds. Click a tile to flip just that one; click the bar at the top to pause
//! or resume the loop.
//!
//! Run it twice and compare what you see:
//!
//! ```text
//! cargo run -p blinc_app_examples --example node_table_repro
//! BLINC_NODE_TABLE=1 cargo run -p blinc_app_examples --example node_table_repro
//! BLINC_MOTION_PROMOTION=1 cargo run -p blinc_app_examples --example node_table_repro
//! ```
//!
//! The first paints each moving subtree as vectors once and moves it with rows applied on the CPU,
//! as the framework does by default. The second moves it with rows of the GPU node table instead.
//! The third bakes it to a texture and blits it, as the framework used to; it leaves labels and
//! images behind and places nested nodes wrongly. Things to look at:
//!
//! - edges and text while a card scales up: a baked texture goes soft, the table stays sharp;
//! - the label on a card that moves, scales or turns: it must stay put on the card;
//! - the shadow and the border of a card that turns or scales;
//! - `FALLBACKS=1` adds the tile marked "falls back": a fade that a tile painted after it lies
//!   over. Rows do not move it, and while it is moving the whole frame is painted in full, so the
//!   other tiles are not moved by rows either. It should look the same in both runs;
//! - anything that pops, flickers or lags one frame behind its card.
//!
//! `RUST_LOG=blinc_app::node_rows=info` prints whether each fast frame was moved by rows.

use blinc_animation::{SharedAnimatedValue, SpringConfig};
use blinc_app::prelude::*;
use blinc_app::windowed::WindowedContext;
use blinc_core::Color;
use blinc_layout::motion::motion;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Once};

/// Whether the loop is paused.
static PAUSED: AtomicBool = AtomicBool::new(false);

/// How long each pose is held, in seconds.
const HOLD: f32 = 2.0;

/// How long each pose is held, from `HOLD_SECS` if it is set: a long one leaves the scenes at
/// rest, which shows what the window costs when nothing moves.
fn hold() -> f32 {
    std::env::var("HOLD_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(HOLD)
}

type Flip = Arc<dyn Fn() + Send + Sync>;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let config = WindowConfig {
        title: format!(
            "Node table repro: {}",
            if table_is_on() {
                "GPU rows"
            } else if promotion_is_on() {
                "baked textures"
            } else {
                "CPU rows"
            }
        ),
        width: 1000,
        height: 760,
        ..Default::default()
    };
    blinc_app::windowed::WindowedApp::run(config, build_ui)
}

/// A 1x1 image, shown scaled up.
const PIXEL: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNg+M/wHwAEAQH/cetH5QAAAABJRU5ErkJggg==";

/// Whether the tiles the node table cannot move are shown too (`FALLBACKS=1`).
fn fallbacks() -> bool {
    std::env::var("FALLBACKS").is_ok_and(|v| v == "1")
}

fn promotion_is_on() -> bool {
    std::env::var("BLINC_MOTION_PROMOTION")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn table_is_on() -> bool {
    std::env::var("BLINC_NODE_TABLE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// A spring-driven value that persists across rebuilds, at rest at `initial`.
fn bound(ctx: &WindowedContext, scene: &str, name: &str, initial: f32) -> SharedAnimatedValue {
    ctx.use_animated_value_for(
        format!("node_table_repro:{scene}:{name}"),
        initial,
        SpringConfig::wobbly(),
    )
}

/// What one scene moves between its two poses: each bound value and the two targets.
struct Pose {
    value: SharedAnimatedValue,
    rest: f32,
    far: f32,
}

/// A way to flip a scene: it alternates between rest and far each time it is called.
fn flipper(poses: Vec<Pose>) -> Flip {
    let far = Arc::new(AtomicBool::new(false));
    Arc::new(move || {
        let now_far = !far.fetch_xor(true, Ordering::Relaxed);
        for pose in &poses {
            let target = if now_far { pose.far } else { pose.rest };
            pose.value.lock().unwrap().set_target(target);
        }
    })
}

fn card(label: Option<&str>) -> Div {
    let card = div()
        .w(100.0)
        .h(56.0)
        .rounded(12.0)
        .bg(Color::rgb(0.2, 0.45, 0.9))
        .border(3.0, Color::rgba(1.0, 1.0, 1.0, 0.9))
        .shadow_params(0.0, 6.0, 12.0, Color::rgba(0.0, 0.0, 0.0, 0.5))
        .flex_row()
        .items_center()
        .justify_center();
    match label {
        Some(label) => card.child(text(label).size(16.0).color(Color::WHITE)),
        None => card,
    }
}

/// The card's own size, so a motion container inside it pivots about the card's centre.
fn holder() -> Div {
    div().w(100.0).h(56.0)
}

fn stage() -> Div {
    div()
        .w(220.0)
        .h(130.0)
        .bg(Color::rgb(0.09, 0.1, 0.13))
        .rounded(10.0)
        .flex_row()
        .items_center()
        .justify_center()
}

/// A tile: a stage under a title, flipped when clicked.
fn tile(title: &str, note: &str, stage: Div, flip: Flip) -> Div {
    div()
        .flex_col()
        .gap(1.0)
        .child(
            text(title)
                .size(13.0)
                .weight(FontWeight::SemiBold)
                .color(Color::WHITE),
        )
        .child(text(note).size(10.0).color(Color::rgb(0.6, 0.65, 0.75)))
        .child(div().on_click(move |_| flip()).child(stage))
}

pub fn build_ui(ctx: &mut WindowedContext) -> impl ElementBuilder + use<> {
    let mut flips: Vec<Flip> = Vec::new();
    let mut tiles: Vec<Div> = Vec::new();

    // 1. Translate.
    {
        let (x, y) = (
            bound(ctx, "translate", "x", 0.0),
            bound(ctx, "translate", "y", 0.0),
        );
        let flip = flipper(vec![
            Pose {
                value: x.clone(),
                rest: 0.0,
                far: 60.0,
            },
            Pose {
                value: y.clone(),
                rest: 0.0,
                far: -24.0,
            },
        ]);
        flips.push(flip.clone());
        let ui = stage()
            .child(holder().child(card(Some("Card")).motion().translate_x(x).translate_y(y)));
        tiles.push(tile("Translate", "card and label move together", ui, flip));
    }

    // 2. Scale.
    {
        let s = bound(ctx, "scale", "s", 1.0);
        let flip = flipper(vec![Pose {
            value: s.clone(),
            rest: 1.0,
            far: 1.7,
        }]);
        flips.push(flip.clone());
        let ui = stage().child(holder().child(card(Some("Scale")).motion().scale(s)));
        tiles.push(tile(
            "Scale",
            "edges and text stay sharp, border and shadow grow",
            ui,
            flip,
        ));
    }

    // 3. Rotate.
    {
        let r = bound(ctx, "rotate", "r", 0.0);
        let flip = flipper(vec![Pose {
            value: r.clone(),
            rest: 0.0,
            far: 50.0,
        }]);
        flips.push(flip.clone());
        let ui = stage().child(holder().child(card(Some("Turn")).motion().rotate(r)));
        tiles.push(tile(
            "Rotate",
            "label, shadow and border turn with the card",
            ui,
            flip,
        ));
    }

    // 4. Fade, with a label: an opacity group with text in it.
    {
        let o = bound(ctx, "fade", "o", 1.0);
        let flip = flipper(vec![Pose {
            value: o.clone(),
            rest: 1.0,
            far: 0.15,
        }]);
        flips.push(flip.clone());
        let ui = stage().child(holder().child(card(Some("Fade")).motion().opacity(o)));
        tiles.push(tile(
            "Fade",
            "the label and the box fade together, as one group",
            ui,
            flip,
        ));
    }

    // 5. All four, no label.
    {
        let x = bound(ctx, "all", "x", 0.0);
        let s = bound(ctx, "all", "s", 1.0);
        let r = bound(ctx, "all", "r", 0.0);
        let o = bound(ctx, "all", "o", 1.0);
        let flip = flipper(vec![
            Pose {
                value: x.clone(),
                rest: 0.0,
                far: -50.0,
            },
            Pose {
                value: s.clone(),
                rest: 1.0,
                far: 1.4,
            },
            Pose {
                value: r.clone(),
                rest: 0.0,
                far: -35.0,
            },
            Pose {
                value: o.clone(),
                rest: 1.0,
                far: 0.4,
            },
        ]);
        flips.push(flip.clone());
        let ui = stage().child(
            holder().child(
                card(None)
                    .motion()
                    .translate_x(x)
                    .scale(s)
                    .rotate(r)
                    .opacity(o),
            ),
        );
        tiles.push(tile(
            "All four",
            "translate, scale, turn and fade at once",
            ui,
            flip,
        ));
    }

    // 6. Nested: the outer one moves, the inner one scales and turns about its own centre.
    {
        let x = bound(ctx, "nested", "x", 0.0);
        let s = bound(ctx, "nested", "s", 1.0);
        let r = bound(ctx, "nested", "r", 0.0);
        let flip = flipper(vec![
            Pose {
                value: x.clone(),
                rest: 0.0,
                far: 60.0,
            },
            Pose {
                value: s.clone(),
                rest: 1.0,
                far: 1.3,
            },
            Pose {
                value: r.clone(),
                rest: 0.0,
                far: 25.0,
            },
        ]);
        flips.push(flip.clone());
        let inner = card(Some("Nested")).motion().scale(s).rotate(r);
        let outer = holder().child(inner).motion().translate_x(x);
        let ui = stage().child(holder().child(outer));
        tiles.push(tile(
            "Nested",
            "an outer move over an inner scale and turn",
            ui,
            flip,
        ));
    }

    // 7. A card moving inside a clipped box.
    {
        let x = bound(ctx, "clipped", "x", -40.0);
        let flip = flipper(vec![Pose {
            value: x.clone(),
            rest: -40.0,
            far: 40.0,
        }]);
        flips.push(flip.clone());
        let clipped = div()
            .w(150.0)
            .h(84.0)
            .rounded(14.0)
            .overflow_clip()
            .bg(Color::rgb(0.16, 0.18, 0.24))
            .flex_row()
            .items_center()
            .justify_center()
            .child(holder().child(card(Some("Clip")).motion().translate_x(x)));
        let ui = stage().child(clipped);
        tiles.push(tile(
            "Clipped",
            "the box clips the card as it slides",
            ui,
            flip,
        ));
    }

    // 7b. A layer effect: a drop shadow the group casts, on a card that moves, scales and turns.
    {
        let x = bound(ctx, "effect", "x", 0.0);
        let s = bound(ctx, "effect", "s", 1.0);
        let r = bound(ctx, "effect", "r", 0.0);
        let flip = flipper(vec![
            Pose {
                value: x.clone(),
                rest: 0.0,
                far: 40.0,
            },
            Pose {
                value: s.clone(),
                rest: 1.0,
                far: 1.25,
            },
            Pose {
                value: r.clone(),
                rest: 0.0,
                far: 20.0,
            },
        ]);
        flips.push(flip.clone());
        let card = card(Some("Effect")).drop_shadow_effect(
            6.0,
            8.0,
            10.0,
            Color::rgba(0.0, 0.0, 0.0, 0.7),
        );
        let ui = stage().child(holder().child(card.motion().translate_x(x).scale(s).rotate(r)));
        tiles.push(tile(
            "Layer effect",
            "a drop shadow the group casts follows the card",
            ui,
            flip,
        ));
    }

    // 7c. An image in a card: above the card's background, growing and moving with it.
    {
        let x = bound(ctx, "image", "x", 0.0);
        let sc = bound(ctx, "image", "s", 1.0);
        let flip = flipper(vec![
            Pose {
                value: x.clone(),
                rest: 0.0,
                far: 40.0,
            },
            Pose {
                value: sc.clone(),
                rest: 1.0,
                far: 1.3,
            },
        ]);
        flips.push(flip.clone());
        let card = card(None).child(blinc_layout::image(PIXEL).size(36.0, 36.0));
        let ui = stage().child(holder().child(card.motion().translate_x(x).scale(sc)));
        tiles.push(tile(
            "Image",
            "the image sits on the card and grows with it",
            ui,
            flip,
        ));
    }

    // 7d. A fade with a tile painted after it, over the card: the group keeps its place in the
    // paint order, which the dynamic batch cannot.
    if fallbacks() {
        let o = bound(ctx, "under", "o", 1.0);
        let flip = flipper(vec![Pose {
            value: o.clone(),
            rest: 1.0,
            far: 0.3,
        }]);
        flips.push(flip.clone());
        let tile_after = div()
            .w(40.0)
            .h(24.0)
            .ml(-15.0)
            .bg(Color::rgb(0.95, 0.85, 0.2));
        let ui = stage()
            .child(holder().child(card(Some("Fade")).motion().opacity(o)))
            .child(tile_after);
        tiles.push(tile(
            "Fade under a tile (falls back)",
            "the yellow tile stays above the card as it fades",
            ui,
            flip,
        ));
    }

    // 8. A list of bound rows, each with its own value.
    {
        let rows: Vec<SharedAnimatedValue> = (0..5)
            .map(|i| bound(ctx, "list", &format!("row{i}"), 0.0))
            .collect();
        let flip = flipper(
            rows.iter()
                .enumerate()
                .map(|(i, v)| Pose {
                    value: v.clone(),
                    rest: 0.0,
                    far: 20.0 + 14.0 * i as f32,
                })
                .collect(),
        );
        flips.push(flip.clone());
        let mut column = div().flex_col().gap(1.0);
        for (i, v) in rows.into_iter().enumerate() {
            let hue = 0.35 + 0.1 * i as f32;
            column = column.child(
                div().w(100.0).h(14.0).child(
                    div()
                        .w(60.0)
                        .h(14.0)
                        .rounded(5.0)
                        .bg(Color::rgb(0.25, hue, 0.9 - 0.1 * i as f32))
                        .motion()
                        .translate_x(v),
                ),
            );
        }
        tiles.push(tile(
            "List",
            "five bound rows, each moving on its own",
            stage().child(column),
            flip,
        ));
    }

    // 9. Text alone in a bound container: it has no primitives of its own.
    {
        let x = bound(ctx, "text", "x", 0.0);
        let r = bound(ctx, "text", "r", 0.0);
        let flip = flipper(vec![
            Pose {
                value: x.clone(),
                rest: 0.0,
                far: 50.0,
            },
            Pose {
                value: r.clone(),
                rest: 0.0,
                far: -20.0,
            },
        ]);
        flips.push(flip.clone());
        let label = div().w(100.0).h(30.0).child(
            motion()
                .translate_x(x)
                .rotate(r)
                .items_center()
                .justify_center()
                .child(text("Just text").size(18.0).color(Color::WHITE)),
        );
        tiles.push(tile(
            "Text only",
            "a bound container with nothing but text",
            stage().child(label),
            flip,
        ));
    }

    // One timer thread flips every scene together. Setting a target wakes the animation
    // scheduler, so the loop does not depend on anything already animating. Started once; later
    // rebuilds find it running and keep their own handles to the same values.
    static LOOP: Once = Once::new();
    let all = flips.clone();
    LOOP.call_once(move || {
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_secs_f32(hold()));
                if PAUSED.load(Ordering::Relaxed) {
                    continue;
                }
                for flip in &all {
                    flip();
                }
            }
        });
    });

    let mode = if table_is_on() {
        "GPU rows: motion-bound nodes are vectors moved by rows of the GPU node table"
    } else if promotion_is_on() {
        "baked textures: motion-bound nodes are baked and blitted (BLINC_MOTION_PROMOTION=1)"
    } else {
        "CPU rows: motion-bound nodes are vectors moved by rows applied on the CPU"
    };

    let mut grid = div().flex_row().flex_wrap().gap(5.0).justify_center();
    for tile in tiles {
        grid = grid.child(tile);
    }

    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgb(0.06, 0.07, 0.09))
        .flex_col()
        .items_center()
        .gap(4.0)
        .p(6.0)
        .child(
            div()
                .on_click(|_| {
                    PAUSED.fetch_xor(true, Ordering::Relaxed);
                })
                .w_full()
                .flex_col()
                .items_center()
                .gap(1.0)
                .py(2.0)
                .rounded(8.0)
                .bg(Color::rgb(0.12, 0.14, 0.2))
                .child(
                    text("Node table repro")
                        .size(22.0)
                        .weight(FontWeight::Bold)
                        .color(Color::WHITE),
                )
                .child(text(mode).size(12.0).color(Color::rgb(0.55, 0.85, 0.6)))
                .child(
                    text("click a tile to flip it; click this bar to pause or resume the loop")
                        .size(11.0)
                        .color(Color::rgb(0.6, 0.65, 0.75)),
                ),
        )
        .child(grid)
}
