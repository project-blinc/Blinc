//! Example
//!
//! A Blinc UI application with desktop, Android, iOS, and HarmonyOS support.
//! Demonstrates blinc_cn components (button, input) on the system light or dark
//! theme, counter interactions and keyframe canvas animations.

use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_cn as cn;
use blinc_cn::{ButtonSize, ButtonVariant, button, cn_bundle, input};
use blinc_core::reactive::State;
use blinc_core::{Brush, DrawContext, Gradient};
use blinc_theme::{ColorToken, ThemeState, detect_system_color_scheme};
use std::f32::consts::PI;
use std::sync::{Arc, Once};

/// A colour of the active theme, read when the UI is built so it follows the system scheme.
fn token(token: ColorToken) -> Color {
    ThemeState::get().color(token)
}

/// A theme colour at the given opacity.
fn with_alpha(color: Color, alpha: f32) -> Color {
    Color::rgba(color.r, color.g, color.b, alpha)
}

/// Install the cn theme and its stylesheet, before the runner starts so the first build has them.
/// The runner keeps a theme that is already set, so this replaces its platform default.
fn install_cn_theme() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        ThemeState::init(
            cn_bundle().with_css(MOBILE_CSS),
            detect_system_color_scheme(),
        )
    });
}

/// The example's own rules over cn's defaults. Card padding is a CSS rule in cn (24 pt), so a
/// 16 pt grouped card is a rule here, which wins over the component layer.
const MOBILE_CSS: &str = r#"
.cn-card { padding: var(--space-4); gap: var(--space-3); }
.demo-card { padding: var(--space-3); align-items: center; }
.list-card { padding: 0; gap: 0; overflow: hidden; }
"#;

/// Spacing methods (`gap`, `p`, `px`, `pt`, `pb`) count in 4 px units; this converts points.
fn u(points: f32) -> f32 {
    points / 4.0
}

/// Horizontal page margin: the 16 pt of the iOS and Material layout grids.
const MARGIN: f32 = 16.0;
/// Padding inside a card.
const CARD_PAD: f32 = 16.0;
/// Space between the two columns of the demo grid.
const GRID_GAP: f32 = 12.0;

/// A grouped section: a small uppercase label over a rounded card, the inset-grouped pattern.
fn section(title: &str, body: Div) -> Div {
    grouped(title, cn::card().w_full().child(body))
}

/// A small uppercase label over `card`.
fn grouped(title: &str, card: cn::Card) -> Div {
    div()
        .w_full()
        .flex_col()
        .gap(u(8.0))
        .child(
            div().w_full().px(u(4.0)).child(
                cn::p(title.to_uppercase())
                    .size(13.0)
                    .color(token(ColorToken::TextSecondary)),
            ),
        )
        .child(card)
}

/// The inside of a card: a column as wide as the card, so text wraps at its edge.
fn card_body() -> Div {
    div().w_full().flex_col().gap(u(12.0))
}

/// Counter button: a cn button at the large size (44 pt or more to tap). Its colours come from
/// the theme through the variant: `Primary` is the `primary` token, `Outline` a border only.
fn counter_button(
    label: &str,
    count: State<i32>,
    delta: i32,
    variant: ButtonVariant,
) -> impl ElementBuilder {
    button(label)
        .variant(variant)
        .size(ButtonSize::Large)
        .on_click(move |_| count.set(count.get() + delta))
}

/// Counter display that reacts to count changes
fn counter_display(count: State<i32>) -> impl ElementBuilder {
    stateful::<NoState>()
        .deps([count.signal_id()])
        .on_state(move |_ctx| {
            div().w_full().flex_row().justify_center().child(
                cn::h1(count.get().to_string())
                    .color(token(ColorToken::TextPrimary))
                    .align(TextAlign::Center),
            )
        })
}

/// Counter demo section
fn counter_section(ctx: &WindowedContext) -> Div {
    let count = ctx.use_state_keyed("count", || 0i32);

    section(
        "Counter",
        card_body()
            .items_center()
            .child(counter_display(count.clone()))
            .child(
                div()
                    .flex_row()
                    .items_center()
                    .justify_center()
                    .gap(u(16.0))
                    .child(counter_button(
                        "\u{2212}",
                        count.clone(),
                        -1,
                        ButtonVariant::Outline,
                    ))
                    .child(counter_button(
                        "+",
                        count.clone(),
                        1,
                        ButtonVariant::Primary,
                    )),
            ),
    )
}

/// Soft-keyboard test section.
///
/// Tap the text field to focus it. On Android the IME pops up via
/// `app.show_soft_input()` from `android.rs`; on iOS the
/// `BlincKeyboardHelper.shared.showKeyboard()` Swift bridge is
/// invoked from `ios.rs` via `dlsym`-resolved
/// `blinc_ios_show_keyboard`. Tap outside (or call
/// `clear_focus()`) to dismiss.
///
/// **iOS prerequisite for soft keyboard**: copy
/// `extensions/blinc_platform_ios/templates/BlincNativeBridge.swift`
/// into your iOS project and add it to the Xcode build target. The
/// runner falls back to a no-op when the symbol isn't present, so
/// the link still succeeds without it — text input still works at
/// the model level (cursor moves, characters insert via hardware
/// keyboard / Bluetooth keyboard) but the soft keyboard won't pop
/// up.
fn keyboard_section(ctx: &WindowedContext) -> Div {
    // Persistent text-input state, keyed so it survives rebuilds.
    let input_state = ctx.use_state_keyed("kbd_test_input", || {
        text_input_state_with_placeholder("Tap to type")
    });

    section(
        "Soft keyboard",
        card_body().child(
            input(&input_state.get())
                .label("Message")
                .placeholder("Tap to type")
                .description("The system keyboard opens when you tap the field.")
                .w_full(),
        ),
    )
}

/// Demo 1: Spinning loader using rotation keyframes
fn spinning_loader_demo(ctx: &WindowedContext, col: f32) -> cn::Card {
    let timeline = ctx.use_animated_timeline();

    let entry_id = timeline.lock().unwrap().configure(|t| {
        let entry = t.add(0, 1000, 0.0, 360.0);
        t.set_loop(-1);
        t.start();
        entry
    });

    let render_timeline = Arc::clone(&timeline);

    demo_card("Spinning Loader", col).child(
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
                    Brush::Solid(with_alpha(token(ColorToken::Primary), alpha)),
                );
            }
        })
        .w(100.0)
        .h(100.0),
    )
}

/// Demo 2: Pulsing dots with staggered keyframes
fn pulsing_dots_demo(ctx: &WindowedContext, col: f32) -> cn::Card {
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

    demo_card("Pulsing Dots", col).child(
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
                    Brush::Solid(with_alpha(token(ColorToken::Accent), opacity)),
                );
            }
        })
        .w(100.0)
        .h(100.0),
    )
}

/// Demo 3: Progress bar with eased fill animation
fn progress_bar_demo(ctx: &WindowedContext, col: f32) -> Div {
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

    div()
        .id("progress-bar-demo")
        .on_click(move |_| {
            click_timeline.lock().unwrap().restart();
        })
        .child(
            demo_card("Progress Bar", col)
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
                            Brush::Solid(token(ColorToken::Border)),
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
                                    token(ColorToken::Primary),
                                    token(ColorToken::Accent),
                                )),
                            );
                        }
                    })
                    .w(col - 2.0 * DEMO_PAD)
                    .h(60.0),
                )
                .child(
                    div()
                        .w_full()
                        .flex_row()
                        .justify_center()
                        .child(cn::caption("Tap to restart")),
                ),
        )
}

/// Demo 4: Bouncing ball with squash and stretch
fn bouncing_ball_demo(ctx: &WindowedContext, col: f32) -> cn::Card {
    let timeline = ctx.use_animated_timeline();

    let entry_id = timeline.lock().unwrap().configure(|t| {
        let y_entry = t.add(0, 800, 0.0, 1.0);
        t.set_loop(-1);
        t.start();
        y_entry
    });

    let render_timeline = Arc::clone(&timeline);

    demo_card("Bouncing Ball", col).child(
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

/// Padding inside a demo card.
const DEMO_PAD: f32 = 12.0;

/// Animation demos section: two columns of equal cards.
fn animation_section(ctx: &WindowedContext) -> Div {
    // Page margins and the section card's padding come off the width before the two columns do.
    let col = ((ctx.width - 2.0 * MARGIN - 2.0 * CARD_PAD - GRID_GAP) / 2.0).floor();
    section(
        "Keyframe animations",
        card_body()
            .child(
                cn::p("Canvas elements animated with keyframes.")
                    .size(15.0)
                    .color(token(ColorToken::TextSecondary)),
            )
            .child(
                div()
                    .w_full()
                    .flex_row()
                    .gap(u(GRID_GAP))
                    .child(spinning_loader_demo(ctx, col))
                    .child(pulsing_dots_demo(ctx, col)),
            )
            .child(
                div()
                    .w_full()
                    .flex_row()
                    .gap(u(GRID_GAP))
                    .child(progress_bar_demo(ctx, col))
                    .child(bouncing_ball_demo(ctx, col)),
            ),
    )
}

/// One inbox row: an initial in a tinted circle, a name over a preview, and the time.
fn inbox_row(name: &str, preview: &str, time: &str, last: bool) -> Div {
    let row = div()
        .w_full()
        .flex_row()
        .items_center()
        .gap(u(12.0))
        .px(u(MARGIN))
        .py(u(10.0))
        .child(
            div()
                .w(40.0)
                .h(40.0)
                .flex_row()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(with_alpha(token(ColorToken::Primary), 0.15))
                .child(
                    cn::p(name.chars().next().unwrap_or('?').to_string())
                        .size(17.0)
                        .weight(FontWeight::SemiBold)
                        .color(token(ColorToken::Primary)),
                ),
        )
        .child(
            div()
                .flex_1()
                .flex_col()
                .gap(u(2.0))
                .child(cn::p(name).size(16.0).weight(FontWeight::SemiBold))
                .child(
                    cn::p(preview)
                        .size(14.0)
                        .color(token(ColorToken::TextSecondary)),
                ),
        )
        .child(
            cn::p(time)
                .size(13.0)
                .color(token(ColorToken::TextSecondary)),
        );
    let mut group = div().w_full().flex_col().child(row);
    if !last {
        // Inset like the iOS list separator: it starts under the text, not the avatar.
        group = group.child(div().w_full().pl(u(MARGIN + 52.0)).child(cn::separator()));
    }
    group
}

/// A long list of rows: enough height to scroll on any phone.
fn inbox_section() -> Div {
    const PEOPLE: [(&str, &str, &str); 24] = [
        (
            "Ada Lovelace",
            "The engine can compose elaborate music",
            "9:41",
        ),
        ("Grace Hopper", "Found the moth in relay 70", "9:12"),
        ("Alan Turing", "Can machines think? Notes attached", "8:55"),
        ("Katherine Johnson", "Trajectory checks are done", "8:30"),
        (
            "Margaret Hamilton",
            "Priority displays worked as designed",
            "Yesterday",
        ),
        ("Linus Torvalds", "Just a hobby, won't be big", "Yesterday"),
        (
            "Barbara Liskov",
            "A subtype must stay substitutable",
            "Yesterday",
        ),
        ("Dennis Ritchie", "The compiler fits on one tape", "Tuesday"),
        (
            "Ken Thompson",
            "Rewrote the shell over the weekend",
            "Tuesday",
        ),
        ("Edsger Dijkstra", "Goto considered, again", "Tuesday"),
        ("Donald Knuth", "Volume 4 is almost ready", "Monday"),
        ("Frances Allen", "Loop optimisation review", "Monday"),
        (
            "John Carmack",
            "Fast inverse square root, explained",
            "Monday",
        ),
        ("Radia Perlman", "Spanning tree, in verse", "Sunday"),
        ("Tim Berners-Lee", "This is for everyone", "Sunday"),
        ("Hedy Lamarr", "Frequency hopping sketch", "Sunday"),
        ("Bjarne Stroustrup", "Zero-cost, as promised", "Saturday"),
        ("Guido van Rossum", "Readability counts", "Saturday"),
        ("Yukihiro Matsumoto", "Optimised for happiness", "Saturday"),
        ("Anders Hejlsberg", "Types make refactors safe", "Friday"),
        ("James Gosling", "Write once, run anywhere", "Friday"),
        ("Brendan Eich", "Ten days in May", "Friday"),
        ("Rich Hickey", "Simple is not easy", "Thursday"),
        ("Joe Armstrong", "Let it crash", "Thursday"),
    ];
    let last = PEOPLE.len() - 1;
    let mut rows = div().w_full().flex_col();
    for (i, (name, preview, time)) in PEOPLE.iter().enumerate() {
        rows = rows.child(inbox_row(name, preview, time, i == last));
    }
    grouped("Inbox", cn::card().w_full().class("list-card").child(rows))
}

/// A demo card: a title over its canvas, `col` points wide.
fn demo_card(title: &str, col: f32) -> cn::Card {
    cn::card().w(col).class("demo-card").child(
        div().w_full().flex_col().items_center().gap(u(8.0)).child(
            cn::p(title)
                .size(14.0)
                .weight(FontWeight::SemiBold)
                .align(TextAlign::Center),
        ),
    )
}

/// Main application UI with scroll container.
///
/// Content sits inside the safe area: below the status bar and notch, above the home indicator.
fn app_ui(ctx: &mut WindowedContext) -> impl ElementBuilder {
    let (top, bottom) = (ctx.safe_top(), ctx.safe_bottom());
    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(token(ColorToken::Background))
        .child(
            scroll().w(ctx.width).h(ctx.height).child(
                div()
                    .w_full()
                    .flex_col()
                    .gap(u(24.0))
                    .px(u(MARGIN))
                    .pt(u(top + MARGIN))
                    .pb(u(bottom + 24.0))
                    // Large title, leading aligned, over a secondary line.
                    .child(
                        div()
                            .w_full()
                            .flex_col()
                            .gap(u(4.0))
                            .child(cn::h1("Blinc"))
                            .child(
                                cn::p("Components, input and animation on mobile.")
                                    .size(15.0)
                                    .color(token(ColorToken::TextSecondary)),
                            ),
                    )
                    .child(counter_section(ctx))
                    .child(keyboard_section(ctx))
                    .child(animation_section(ctx))
                    .child(inbox_section()),
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

    WindowedApp::run_with_theme(
        config,
        cn_bundle().with_css(MOBILE_CSS),
        detect_system_color_scheme(),
        |ctx| app_ui(ctx),
    )
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

    // Wire up the JNI bridge to the Kotlin BlincNativeBridge object.
    // MainActivity.onCreate has already called BlincNativeBridge.registerDefaults
    // on the JVM side, so this just attaches the platform adapter on the Rust side.
    if let Err(e) = blinc_platform_android::init_android_native_bridge(&app) {
        log::warn!("Failed to init Android native bridge: {}", e);
    }

    // The cn theme goes in before the first frame, so its stylesheet is drained with the first build.
    install_cn_theme();
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
    // Filter out the chatty wgpu/naga shader-compilation crates so the
    // Xcode console only shows blinc + app traces. Without this filter
    // every shader compile dumps thousands of lines of `naga::front`
    // type-resolution debug output which drowns out touch-event traces.
    let filter = tracing_subscriber::EnvFilter::new(
        "info,\
         blinc_layout=debug,\
         blinc_app=debug,\
         blinc_animation=debug,\
         blinc_layout::widgets::text_input=debug,\
         blinc_layout::widgets::text_edit=debug,\
         wgpu=warn,\
         wgpu_core=warn,\
         wgpu_hal=warn,\
         naga=warn",
    );
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();

    eprintln!("[Blinc] ios_app_init called - registering UI builder");

    // The cn theme goes in before the first frame, so its stylesheet is drained with the first build.
    install_cn_theme();
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
