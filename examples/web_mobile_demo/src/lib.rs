//! Blinc web example — the same UI as `mobile/example` running on wasm32.
//!
//! This is a near-verbatim port of [`mobile/example/src/main.rs`](../../mobile/example/src/main.rs):
//! the counter section, the soft-keyboard test section, and all four
//! keyframe canvas animation demos (spinning loader, pulsing dots,
//! progress bar, bouncing ball) are identical to the desktop / iOS /
//! Android build. Only the platform entry point and font loading
//! differ — everything inside `app_ui` and the helper builders is
//! shared with the mobile example by design.
//!
//! ## Build
//!
//! ```bash
//! cd examples/web_mobile_demo
//! wasm-pack build --target web --release
//! ./serve.sh
//! # open http://localhost:8000/
//! ```
//!
//! Requires Chrome / Edge ≥ 113 (or any browser with WebGPU enabled).
//!
//! ## Why a near-verbatim copy
//!
//! The point of this example is to prove that a real UI built against
//! Blinc's widget API on mobile runs unchanged on the web target. The
//! shared widgets exercised here are: `text_input` (which routes
//! through the soft keyboard plumbing on mobile and the browser
//! `compositionstart` events on web), `scroll`, `canvas`, the
//! keyframe animation timeline (`use_animated_timeline`), and the
//! `Stateful<ButtonState>` hover/press FSM. Anything that breaks on
//! the web target should be diagnosed by comparing the runner glue,
//! not the UI code.

#![cfg(target_arch = "wasm32")]

use blinc_app::prelude::*;
use blinc_app::web::WebApp;
use blinc_app::windowed::WindowedContext;
use blinc_core::reactive::State;
use blinc_core::{Brush, DrawContext, Gradient};
use std::f32::consts::PI;
use std::sync::Arc;
use wasm_bindgen::prelude::*;

/// Bundled font from `assets/fonts/` at the workspace root.
/// Browsers can't hand wgpu their system fonts, so the font bytes
/// are included in the wasm binary via `include_bytes!`.
const ARIAL_TTF: &[u8] = include_bytes!("../../../assets/fonts/Arial.ttf");

/// Counter button with stateful hover/press states. Identical to
/// the mobile example version.
fn counter_button(label: &str, count: State<i32>, delta: i32) -> impl ElementBuilder + use<> {
    let label = label.to_string();

    let count = count.clone();
    stateful::<ButtonState>()
        .on_state(move |ctx| {
            let bg = match ctx.state() {
                ButtonState::Idle => Color::rgba(0.3, 0.3, 0.4, 1.0),
                ButtonState::Hovered => Color::rgba(0.4, 0.4, 0.5, 1.0),
                ButtonState::Pressed => Color::rgba(0.2, 0.2, 0.3, 1.0),
                ButtonState::Disabled => Color::rgba(0.2, 0.2, 0.2, 0.5),
            };

            div()
                .w(80.0)
                .h(50.0)
                .rounded(8.0)
                .bg(bg)
                .items_center()
                .justify_center()
                .cursor(CursorStyle::Pointer)
                .child(text(&label).size(24.0).color(Color::WHITE))
        })
        .on_click(move |_| {
            count.set(count.get() + delta);
        })
}

/// Counter display that reacts to count changes.
fn counter_display(count: State<i32>) -> impl ElementBuilder + use<> {
    stateful::<NoState>()
        .deps([count.signal_id()])
        .on_state(move |_ctx| {
            div().child(
                text(format!("Count: {}", count.get()))
                    .size(48.0)
                    .color(Color::rgba(0.4, 0.8, 1.0, 1.0))
                    .align(TextAlign::Center),
            )
        })
}

/// Counter demo section.
fn counter_section(ctx: &WindowedContext) -> Div {
    let count = ctx.use_state_keyed("count", || 0i32);

    section_card("Counter Demo")
        .child(counter_display(count.clone()))
        .child(
            div()
                .flex_row()
                .gap(16.0)
                .child(counter_button("-", count.clone(), -1))
                .child(counter_button("+", count.clone(), 1)),
        )
}

/// Soft-keyboard test section.
///
/// On the web target, focusing the field doesn't pop a soft keyboard
/// the way mobile does — desktop browsers don't *have* a soft
/// keyboard, and mobile browsers' on-screen keyboard only appears
/// when an actual `<input>` or `contenteditable` element is focused.
/// The Blinc canvas is a single `<canvas>`, so the browser has no
/// way to know that the user just focused a text widget inside the
/// canvas. The text input itself still works (typing on a hardware
/// keyboard inserts characters, the cursor moves, selection works,
/// double-tap selects a word, etc.) — the *only* missing piece is
/// the OS soft keyboard popup.
///
/// We keep the section here unchanged because everything *inside*
/// it (the input widget, the placeholder rendering, the focus
/// state machine) does run on web and is worth exercising.
fn keyboard_section(ctx: &WindowedContext) -> Div {
    let input_state = ctx.use_state_keyed("kbd_test_input", || {
        text_input_state_with_placeholder("Tap to focus and type…")
    });

    section_card("Keyboard Input Test")
        .child(
            text("Tap the field — type with your keyboard.")
                .size(13.0)
                .align(TextAlign::Center)
                .color(Color::rgba(0.7, 0.7, 0.78, 1.0)),
        )
        .child(
            div().w(300.0).child(
                text_input(&input_state.get())
                    .w_full()
                    .text_color(Color::WHITE)
                    .placeholder_color(Color::rgba(0.55, 0.55, 0.62, 1.0)),
            ),
        )
}

/// Demo 1: Spinning loader using rotation keyframes.
fn spinning_loader_demo(ctx: &WindowedContext) -> Div {
    let timeline = ctx.use_animated_timeline();

    let entry_id = timeline.lock().unwrap().configure(|t| {
        let entry = t.add(0, 1000, 0.0, 360.0);
        t.set_loop(-1);
        t.start();
        entry
    });

    let render_timeline = Arc::clone(&timeline);

    demo_card("Spinning Loader").child(
        canvas(move |ctx: &mut dyn DrawContext, bounds| {
            let timeline = render_timeline.lock().unwrap();
            let angle_deg = timeline.get(entry_id).unwrap_or(0.0);
            let angle_rad = angle_deg * PI / 180.0;

            let cx = bounds.width / 2.0;
            let cy = bounds.height / 2.0;
            let radius = 30.0;
            let thickness = 4.0;

            let arc_length = PI * 1.5;
            let segments = 32;

            for i in 0..segments {
                let t1 = i as f32 / segments as f32;
                let t2 = (i + 1) as f32 / segments as f32;

                let a1 = angle_rad + t1 * arc_length;
                let a2 = angle_rad + t2 * arc_length;

                let x1 = cx + radius * a1.cos();
                let y1 = cy + radius * a1.sin();
                let _x2 = cx + radius * a2.cos();
                let _y2 = cy + radius * a2.sin();

                let dx = _x2 - x1;
                let dy = _y2 - y1;
                let len = (dx * dx + dy * dy).sqrt();

                let alpha = 0.3 + 0.7 * t1;

                ctx.fill_rect(
                    Rect::new(
                        x1 - thickness / 2.0,
                        y1 - thickness / 2.0,
                        len + thickness,
                        thickness,
                    ),
                    blinc_core::CornerRadius::uniform(thickness / 2.0),
                    Brush::Solid(Color::rgba(0.4, 0.8, 1.0, alpha)),
                );
            }
        })
        .w(100.0)
        .h(100.0),
    )
}

/// Demo 2: Pulsing dots with staggered keyframes.
fn pulsing_dots_demo(ctx: &WindowedContext) -> Div {
    let timelines: Vec<SharedAnimatedTimeline> = (0..3)
        .map(|i| ctx.use_animated_timeline_for(format!("pulsing_dot_{}", i)))
        .collect();

    let entry_ids: Vec<_> = timelines
        .iter()
        .enumerate()
        .map(|(i, timeline)| {
            timeline
                .lock()
                .unwrap()
                .configure(|t: &mut AnimatedTimeline| {
                    let offset = i as i32 * 200;
                    let scale_entry = t.add(offset, 600, 0.5, 1.0);
                    let opacity_entry = t.add(offset, 600, 0.3, 1.0);
                    t.set_loop(-1);
                    t.start();
                    (scale_entry, opacity_entry)
                })
        })
        .collect();

    let timelines_clone: Vec<_> = timelines.iter().map(Arc::clone).collect();

    demo_card("Pulsing Dots").child(
        canvas(move |ctx: &mut dyn DrawContext, bounds| {
            let cx = bounds.width / 2.0;
            let cy = bounds.height / 2.0;
            let dot_radius = 8.0;
            let spacing = 25.0;

            for (i, (timeline, (scale_entry, opacity_entry))) in
                timelines_clone.iter().zip(entry_ids.iter()).enumerate()
            {
                let tl = timeline.lock().unwrap();
                let scale = tl.get(*scale_entry).unwrap_or(1.0);
                let opacity = tl.get(*opacity_entry).unwrap_or(1.0);

                let x = cx + (i as f32 - 1.0) * spacing;
                let r = dot_radius * scale;

                ctx.fill_rect(
                    Rect::new(x - r, cy - r, r * 2.0, r * 2.0),
                    blinc_core::CornerRadius::uniform(r),
                    Brush::Solid(Color::rgba(0.4, 1.0, 0.8, opacity)),
                );
            }
        })
        .w(100.0)
        .h(100.0),
    )
}

/// Demo 3: Progress bar with eased fill animation.
fn progress_bar_demo(ctx: &WindowedContext) -> Div {
    let timeline = ctx.use_animated_timeline();

    let entry_id = timeline
        .lock()
        .unwrap()
        .configure(|t| t.add(0, 2000, 0.0, 1.0));

    let render_timeline = Arc::clone(&timeline);
    let click_timeline = Arc::clone(&timeline);
    let ready_timeline = Arc::clone(&timeline);

    ctx.query("progress-bar-demo").on_ready(move |_| {
        ready_timeline.lock().unwrap().start();
    });

    demo_card("Progress Bar")
        .id("progress-bar-demo")
        .child(
            canvas(move |ctx: &mut dyn DrawContext, bounds| {
                let timeline = render_timeline.lock().unwrap();
                let progress_val = timeline.get(entry_id).unwrap_or(0.0);

                let bar_width = bounds.width - 20.0;
                let bar_height = 12.0;
                let bar_x = 10.0;
                let bar_y = (bounds.height - bar_height) / 2.0;

                // Background
                ctx.fill_rect(
                    Rect::new(bar_x, bar_y, bar_width, bar_height),
                    blinc_core::CornerRadius::uniform(6.0),
                    Brush::Solid(Color::rgba(0.2, 0.2, 0.25, 1.0)),
                );

                // Filled portion
                let fill_width = bar_width * progress_val;
                if fill_width > 0.0 {
                    ctx.fill_rect(
                        Rect::new(bar_x, bar_y, fill_width, bar_height),
                        blinc_core::CornerRadius::uniform(6.0),
                        Brush::Gradient(Gradient::linear(
                            Point::new(bar_x, bar_y),
                            Point::new(bar_x + fill_width, bar_y),
                            Color::rgba(0.4, 0.8, 1.0, 1.0),
                            Color::rgba(0.6, 0.4, 1.0, 1.0),
                        )),
                    );
                }
            })
            .w(150.0)
            .h(60.0),
        )
        .child(
            text("Tap to restart")
                .size(12.0)
                .color(Color::rgba(0.5, 0.5, 0.5, 1.0)),
        )
        .on_click(move |_| {
            click_timeline.lock().unwrap().restart();
        })
}

/// Demo 4: Bouncing ball with squash and stretch.
fn bouncing_ball_demo(ctx: &WindowedContext) -> Div {
    let timeline = ctx.use_animated_timeline();

    let entry_id = timeline.lock().unwrap().configure(|t| {
        let y_entry = t.add(0, 800, 0.0, 1.0);
        t.set_loop(-1);
        t.start();
        y_entry
    });

    let render_timeline = Arc::clone(&timeline);

    demo_card("Bouncing Ball").child(
        canvas(move |ctx: &mut dyn DrawContext, bounds| {
            let timeline = render_timeline.lock().unwrap();
            let t = timeline.get(entry_id).unwrap_or(0.0);

            let bounce_height = 50.0;
            let ground_y = bounds.height - 25.0;

            // Simple parabolic bounce
            let y = if t < 0.5 {
                let fall_t = t * 2.0;
                ground_y - bounce_height * (1.0 - fall_t * fall_t)
            } else {
                let rise_t = (t - 0.5) * 2.0;
                ground_y - bounce_height * (1.0 - (1.0 - rise_t) * (1.0 - rise_t))
            };

            // Squash/stretch based on velocity
            let (scale_x, scale_y) = if !(0.45..=0.55).contains(&t) {
                (0.9, 1.1)
            } else {
                (1.2, 0.8)
            };

            let cx = bounds.width / 2.0;
            let radius = 15.0;

            // Draw shadow
            let shadow_scale = 1.0 - (ground_y - y) / bounce_height * 0.5;
            let shadow_width = radius * 2.0 * shadow_scale;
            let shadow_height = radius * 0.3 * 2.0 * shadow_scale;
            ctx.fill_rect(
                Rect::new(
                    cx - shadow_width / 2.0,
                    ground_y + 2.0,
                    shadow_width,
                    shadow_height,
                ),
                blinc_core::CornerRadius::uniform(shadow_height / 2.0),
                Brush::Solid(Color::rgba(0.0, 0.0, 0.0, 0.3 * shadow_scale)),
            );

            // Draw ball with squash/stretch
            let ball_width = radius * 2.0 * scale_x;
            let ball_height = radius * 2.0 * scale_y;
            ctx.fill_rect(
                Rect::new(
                    cx - ball_width / 2.0,
                    y - ball_height / 2.0,
                    ball_width,
                    ball_height,
                ),
                blinc_core::CornerRadius::uniform(ball_height.min(ball_width) / 2.0),
                Brush::Gradient(Gradient::linear(
                    Point::new(cx - ball_width / 2.0, y - ball_height / 2.0),
                    Point::new(cx + ball_width / 2.0, y + ball_height / 2.0),
                    Color::rgba(1.0, 0.5, 0.3, 1.0),
                    Color::rgba(0.9, 0.3, 0.2, 1.0),
                )),
            );
        })
        .w(100.0)
        .h(120.0),
    )
}

/// Animation demos section.
fn animation_section(ctx: &WindowedContext) -> Div {
    section_card("Keyframe Animations")
        .child(
            text("Canvas elements with multi-property keyframe animations")
                .size(16.0)
                .color(Color::rgba(0.6, 0.6, 0.7, 1.0))
                .align(TextAlign::Center),
        )
        .child(
            div()
                .flex_row()
                .gap(10.0)
                .flex_wrap()
                .justify_center()
                .child(spinning_loader_demo(ctx))
                .child(pulsing_dots_demo(ctx)),
        )
        .child(
            div()
                .flex_row()
                .gap(10.0)
                .flex_wrap()
                .justify_center()
                .child(progress_bar_demo(ctx))
                .child(bouncing_ball_demo(ctx)),
        )
}

/// Helper to create a section card.
fn section_card(title: &str) -> Div {
    div()
        .w_full()
        .flex_col()
        .gap(6.0)
        .py(5.0)
        .px(8.0)
        .bg(Color::rgba(0.12, 0.12, 0.17, 1.0))
        .rounded(16.0)
        .items_center()
        .child(
            div().items_center().child(
                text(title)
                    .size(24.0)
                    .align(TextAlign::Center)
                    .weight(FontWeight::Bold)
                    .color(Color::WHITE)
                    .no_wrap(),
            ),
        )
}

/// Helper to create a demo card.
fn demo_card(title: &str) -> Div {
    div()
        .w(170.0)
        .flex_col()
        .gap(5.0)
        .py(8.0)
        .px(4.0)
        .bg(Color::rgba(0.18, 0.18, 0.23, 1.0))
        .rounded(12.0)
        .items_center()
        .child(
            text(title)
                .size(14.0)
                .weight(FontWeight::SemiBold)
                .color(Color::WHITE),
        )
}

/// Main application UI with scroll container. Identical layout to
/// the mobile example, just renamed from `app_ui` to `build_ui` to
/// match the existing web example convention.
fn build_ui(ctx: &mut WindowedContext) -> Div {
    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.08, 0.08, 0.12, 1.0))
        .child(
            scroll().w(ctx.width).h(ctx.height).child(
                div()
                    .w_full()
                    .flex_col()
                    .items_center()
                    .gap(4.0)
                    .px(8.0)
                    .py(15.0)
                    // Header
                    .child(
                        text("Blinc Web Mobile Example")
                            .align(TextAlign::Center)
                            .size(28.0)
                            .weight(FontWeight::Bold)
                            .color(Color::WHITE),
                    )
                    .child(
                        text("Scroll down for more demos")
                            .size(14.0)
                            .color(Color::rgba(0.5, 0.5, 0.6, 1.0)),
                    )
                    // Counter section
                    .child(counter_section(ctx))
                    // Soft-keyboard test section
                    .child(keyboard_section(ctx))
                    // Animation section
                    .child(animation_section(ctx))
                    // Footer spacer
                    .child(div().h(20.0)),
            ),
        )
}

/// wasm-bindgen entry point. The `start` attribute makes this run
/// automatically when the browser loads the generated `.js` shim.
#[wasm_bindgen(start)]
pub fn _start() {
    console_error_panic_hook::set_once();

    tracing_wasm::set_as_global_default_with_config(
        tracing_wasm::WASMLayerConfigBuilder::new()
            .set_max_level(tracing::Level::INFO)
            .build(),
    );

    wasm_bindgen_futures::spawn_local(async {
        let result = WebApp::run_with_setup(
            "blinc-canvas",
            |app| {
                let faces = app.load_font_data(ARIAL_TTF.to_vec());
                web_sys::console::log_1(
                    &format!(
                        "blinc_web_mobile_demo: registered {faces} font face(s) from Arial.ttf"
                    )
                    .into(),
                );
            },
            build_ui,
        )
        .await;

        if let Err(e) = result {
            web_sys::console::error_1(
                &format!("blinc_web_mobile_demo: WebApp::run failed: {e}").into(),
            );
        }
    });
}
