//! Example
//!
//! A Blinc UI application with desktop, Android, iOS, and HarmonyOS support.
//! Demonstrates counter interactions and keyframe canvas animations.

use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_core::reactive::State;
use blinc_core::{Brush, DrawContext, Gradient};
use std::f32::consts::PI;
use std::sync::Arc;

/// Counter button with stateful hover/press states
fn counter_button(label: &str, count: State<i32>, delta: i32) -> impl ElementBuilder {
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

/// Counter display that reacts to count changes
fn counter_display(count: State<i32>) -> impl ElementBuilder {
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

/// Counter demo section
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

/// Demo 1: Spinning loader using rotation keyframes
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

/// Demo 2: Pulsing dots with staggered keyframes
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

/// Demo 3: Progress bar with eased fill animation
fn progress_bar_demo(ctx: &WindowedContext) -> Div {
    let timeline = ctx.use_animated_timeline();

    let entry_id = timeline.lock().unwrap().configure(|t| {
        let entry = t.add(0, 2000, 0.0, 1.0);
        entry
    });

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

/// Demo 4: Bouncing ball with squash and stretch
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
            let (scale_x, scale_y) = if t < 0.45 || t > 0.55 {
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

/// Animation demos section
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

/// Helper to create a section card
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

/// Helper to create a demo card
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

/// Main application UI with scroll container
fn app_ui(ctx: &mut WindowedContext) -> impl ElementBuilder {
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
                        text("Blinc Mobile Example")
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
                    // Animation section
                    .child(animation_section(ctx))
                    // Footer spacer
                    .child(div().h(20.0)),
            ),
        )
}

// =============================================================================
// Desktop Entry Point
// =============================================================================

#[cfg(not(any(target_os = "android", target_os = "ios", target_env = "ohos")))]
fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = WindowConfig {
        title: "Blinc Mobile Example".to_string(),
        width: 400,
        height: 700,
        ..Default::default()
    };

    WindowedApp::run(config, |ctx| app_ui(ctx))
}

// =============================================================================
// Android Entry Point
// =============================================================================

#[cfg(target_os = "android")]
use android_activity::AndroidApp;

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: AndroidApp) {
    use android_logger::Config;
    use log::LevelFilter;

    android_logger::init_once(
        Config::default()
            .with_max_level(LevelFilter::Info)
            .with_tag("example"),
    );

    log::info!("Starting example on Android");

    blinc_app::AndroidApp::run(app, |ctx| app_ui(ctx)).expect("Failed to run Android app");
}

#[cfg(target_os = "android")]
fn main() {}

// =============================================================================
// iOS Entry Point
// =============================================================================

#[cfg(target_os = "ios")]
fn main() {}

/// iOS initialization function - called from Swift during app launch
///
/// This registers the Rust UI builder so that each frame can build the UI.
/// Must be called before blinc_create_context.
#[cfg(target_os = "ios")]
#[no_mangle]
pub extern "C" fn ios_app_init() {
    use std::io::Write;
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_writer(std::io::stderr)
        .try_init();

    eprintln!("[Blinc] ios_app_init called - registering UI builder");

    blinc_app::ios::register_rust_ui_builder(|ctx| app_ui(ctx));

    eprintln!("[Blinc] UI builder registered");
}

// =============================================================================
// HarmonyOS Entry Point
// =============================================================================

#[cfg(target_env = "ohos")]
fn main() {
    // HarmonyOS uses N-API callbacks from XComponent
    // The actual initialization happens via napi_register_module
    // This main() is a placeholder for the cdylib entry
}

/// N-API module export for HarmonyOS
/// Called when the native module is loaded by ArkTS
#[cfg(target_env = "ohos")]
#[no_mangle]
pub extern "C" fn napi_register_module() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    tracing::info!("Blinc HarmonyOS module registered");

    // TODO: Register N-API functions for XComponent callbacks
    // blinc_platform_harmony::napi_bridge::register_module()
}
