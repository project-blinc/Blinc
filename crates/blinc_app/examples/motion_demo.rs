//! Motion Container Demo
//!
//! Demonstrates the motion() element for declarative enter/exit animations:
//! - Single element with fade/scale animations
//! - Staggered list animations with configurable delays
//! - Different stagger directions (forward, reverse, from center)
//! - Various animation presets (fade, scale, slide, bounce, pop)
//! - Pull-to-refresh with FSM + AnimatedValue for smooth drag animation
//! - BlincComponent derive macro for type-safe animation hooks
//!
//! Note: Enter/exit animations require RenderTree integration (pending).
//! This example showcases the API design and stagger delay calculations.
//!
//! Run with: cargo run -p blinc_app --example motion_demo --features windowed

use blinc_animation::{AnimationPreset, SpringConfig};
use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_core::Color;
use blinc_layout::motion::{motion, StaggerConfig};
use blinc_layout::prelude::stateful_from_handle;
use blinc_layout::widgets::scroll::Scroll;
use blinc_theme::theme;
use std::sync::{Arc, Mutex};

/// Component for the pull-to-refresh demo.
/// The BlincComponent derive generates type-safe animation hooks.
/// Fields marked with #[animation] generate SharedAnimatedValue accessors.
#[derive(BlincComponent)]
struct PullToRefresh {
    /// Y offset for dragging content down
    #[animation]
    content_offset: f32,
    /// Scale of the refresh icon
    #[animation]
    icon_scale: f32,
    /// Opacity of the refresh icon
    #[animation]
    icon_opacity: f32,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let config = WindowConfig {
        title: "Motion Container Demo".to_string(),
        width: 900,
        height: 700,
        fullscreen: true,
        ..Default::default()
    };

    WindowedApp::run(config, |ctx| build_ui(ctx))
}

fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    let theme = ThemeState::get();
    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(theme.color(ColorToken::Background).with_alpha(0.8))
        .flex_col()
        .items_center()
        .gap(10.0)
        .p(10.0)
        .child(
            text("Motion Container Demo")
                .size(28.0)
                .weight(FontWeight::Bold)
                .color(Color::WHITE),
        )
        .child(
            text("Declarative enter/exit animations with stagger support")
                .size(14.0)
                .color(Color::rgba(0.6, 0.6, 0.7, 1.0)),
        )
        .child(
            scroll().w_full().h(ctx.height).justify_center().child(
                div()
                    .flex_col()
                    .gap(10.0)
                    .items_center()
                    .child(
                        div()
                            .w_full()
                            .flex_row()
                            .justify_center()
                            .gap(5.0)
                            .flex_wrap()
                            .child(pull_to_refresh_demo(ctx))
                            .child(single_element_demo())
                            .child(stagger_forward_demo())
                            .child(stagger_reverse_demo())
                            .child(stagger_center_demo()),
                    )
                    .child(api_showcase()),
            ),
        )
}

/// Demo 1: Single element with fade + scale animation
fn single_element_demo() -> Div {
    demo_card("Single Element", "fade_in + scale_in").child(
        // motion() wraps the content with enter/exit animations
        motion()
            .items_center()
            .justify_center()
            .fade_in(600)
            .scale_in(600)
            .child(
                div()
                    .w(80.0)
                    .h(80.0)
                    .bg(Color::rgba(0.4, 0.7, 1.0, 1.0))
                    .rounded(8.0)
                    .items_center()
                    .justify_center()
                    .child(text("Content").text_center().size(12.0).color(Color::WHITE)),
            ),
    )
}

/// Demo 2: Staggered list (forward direction)
fn stagger_forward_demo() -> Div {
    let items = vec!["Item 1", "Item 2", "Item 3", "Item 4", "Item 5"];

    demo_card("Stagger Forward", "delay: 300ms").child(
        motion()
            .gap(4.0) // Add gap between staggered items
            .stagger(StaggerConfig::new(300, AnimationPreset::fade_in(800)))
            .children(items.iter().map(|item| list_item(item))),
    )
}

/// Demo 3: Staggered list (reverse direction)
fn stagger_reverse_demo() -> Div {
    let items = vec!["Item 1", "Item 2", "Item 3", "Item 4", "Item 5"];

    demo_card("Stagger Reverse", "delay: 300ms").child(
        motion()
            .gap(4.0) // Add gap between staggered items
            .stagger(StaggerConfig::new(300, AnimationPreset::fade_in(800)).reverse())
            .children(items.iter().map(|item| list_item(item))),
    )
}

/// Demo 4: Staggered list (from center)
fn stagger_center_demo() -> Div {
    let items = vec!["Item 1", "Item 2", "Item 3", "Item 4", "Item 5"];

    demo_card("Stagger Center", "delay: 300ms").child(
        motion()
            .gap(4.0) // Add gap between staggered items
            .stagger(StaggerConfig::new(300, AnimationPreset::fade_in(800)).from_center())
            .children(items.iter().map(|item| list_item(item))),
    )
}

// ============================================================================
// Pull-to-Refresh Demo: stateful() for drag + motion() for animation
// ============================================================================

/// FSM states for pull-to-refresh gesture
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
enum PullState {
    #[default]
    Idle,
    Pulling,    // User is dragging down
    Armed,      // Pulled past threshold, ready to refresh
    Refreshing, // Refresh in progress
}

impl blinc_layout::prelude::StateTransitions for PullState {
    fn on_event(&self, event: u32) -> Option<Self> {
        use blinc_core::events::event_types;
        match (self, event) {
            // Pointer events for state transitions
            (PullState::Idle, event_types::POINTER_DOWN) => Some(PullState::Pulling),
            (PullState::Pulling, event_types::POINTER_UP) => Some(PullState::Idle),
            (PullState::Armed, event_types::POINTER_UP) => Some(PullState::Refreshing),
            // Click in refreshing state resets to idle (simulates refresh complete)
            (PullState::Refreshing, event_types::POINTER_UP) => Some(PullState::Idle),
            _ => None,
        }
    }
}

/// Demo 5: Pull-to-refresh with stack() + motion()
///
/// Architecture:
/// - stack() = layers refresh icon (bottom) and content (top)
/// - motion() wraps content to translate it down, revealing the icon
/// - motion() wraps refresh icon to scale it up when armed/refreshing
/// - stateful() handles FSM for pointer events and state transitions
fn pull_to_refresh_demo(ctx: &WindowedContext) -> Div {
    // AnimatedValues using BlincComponent derive macro for type-safe hooks
    // Each f32 field in PullToRefresh struct gets a use_<field_name> method
    // that returns SharedAnimatedValue
    let content_offset_y = PullToRefresh::use_content_offset(ctx, 0.0, SpringConfig::wobbly());

    let icon_scale = PullToRefresh::use_icon_scale(ctx, 0.5, SpringConfig::snappy());

    let icon_opacity = PullToRefresh::use_icon_opacity(ctx, 0.0, SpringConfig::snappy());

    // Track start Y position for drag calculation
    let drag_start_y = Arc::new(Mutex::new(0.0f32));

    // Clones for closures
    let content_offset_on_state = Arc::clone(&content_offset_y);
    let content_offset_on_move = Arc::clone(&content_offset_y);
    let icon_scale_on_state = Arc::clone(&icon_scale);
    let icon_scale_on_move = Arc::clone(&icon_scale);
    let icon_opacity_on_state = Arc::clone(&icon_opacity);
    let icon_opacity_on_move = Arc::clone(&icon_opacity);
    let drag_start_down = Arc::clone(&drag_start_y);
    let drag_start_move = Arc::clone(&drag_start_y);

    const MAX_PULL: f32 = 80.0;
    const ARMED_THRESHOLD: f32 = 50.0;

    // Get shared FSM state handle
    let pull_state = ctx.use_state_for("pull_refresh", PullState::Idle);
    let pull_state_move = pull_state.clone();

    demo_card("Pull to Refresh", "stack + motion").child(
        // stateful_from_handle() = container with FSM state transitions (using external state handle)
        #[allow(deprecated)]
        stateful_from_handle(pull_state)
            .w(160.0)
            .h(130.0)
            .rounded(8.0)
            .overflow_clip() // Clip children to container bounds
            .bg(Color::rgba(0.1, 0.1, 0.12, 1.0))
            .on_state(move |state: &PullState, container: &mut Div| {
                // Update container color based on state
                let bg = match *state {
                    PullState::Idle => Color::rgba(0.1, 0.1, 0.12, 1.0),
                    PullState::Pulling => Color::rgba(0.12, 0.12, 0.16, 1.0),
                    PullState::Armed => Color::rgba(0.1, 0.15, 0.12, 1.0),
                    PullState::Refreshing => Color::rgba(0.1, 0.12, 0.18, 1.0),
                };
                container.merge(div().bg(bg));

                // Animate based on state
                match *state {
                    PullState::Idle => {
                        // Spring content back up, hide icon
                        content_offset_on_state.lock().unwrap().set_target(0.0);
                        icon_scale_on_state.lock().unwrap().set_target(0.5);
                        icon_opacity_on_state.lock().unwrap().set_target(0.0);
                    }
                    PullState::Refreshing => {
                        // Hold content down, show spinning icon
                        content_offset_on_state.lock().unwrap().set_target(40.0);
                        icon_scale_on_state.lock().unwrap().set_target(1.2);
                        icon_opacity_on_state.lock().unwrap().set_target(1.0);
                    }
                    _ => {}
                }
            })
            // Capture start Y on mouse down
            .on_mouse_down(move |ctx| {
                *drag_start_down.lock().unwrap() = ctx.mouse_y;
            })
            // Mouse move updates offset when in Pulling/Armed state
            .on_mouse_move(move |ctx| {
                let state = pull_state_move.lock().unwrap().state;
                if state == PullState::Pulling || state == PullState::Armed {
                    let start_y = *drag_start_move.lock().unwrap();
                    let delta_y = (ctx.mouse_y - start_y).max(0.0).min(MAX_PULL);

                    // Content follows drag directly
                    content_offset_on_move
                        .lock()
                        .unwrap()
                        .set_immediate(delta_y);

                    // Icon scales and fades based on pull progress
                    let progress = delta_y / ARMED_THRESHOLD;
                    let scale = 0.5 + (progress.min(1.0) * 0.5); // 0.5 -> 1.0
                    let opacity = progress.min(1.0);
                    icon_scale_on_move.lock().unwrap().set_immediate(scale);
                    icon_opacity_on_move.lock().unwrap().set_immediate(opacity);
                }
            })
            // Stack: refresh icon (bottom) + content (top)
            .child(
                stack()
                    .w(160.0)
                    .h(130.0)
                    .child(
                        // Bottom layer: Refresh icon (centered, hidden behind content)
                        div()
                            .w(160.0)
                            .h(50.0)
                            .items_center()
                            .justify_center()
                            .child(
                                motion()
                                    .scale(icon_scale.clone())
                                    .opacity(icon_opacity.clone())
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        div()
                                            .w(32.0)
                                            .h(32.0)
                                            .rounded(16.0)
                                            .bg(Color::rgba(0.3, 0.6, 1.0, 1.0))
                                            .items_center()
                                            .justify_center()
                                            .child(text("↻").size(18.0).color(Color::WHITE)),
                                    ),
                            ),
                    )
                    .child(
                        // Top layer: Content area (slides down to reveal icon)
                        // Has opaque background to cover refresh icon when at rest
                        motion().translate_y(content_offset_y.clone()).child(
                            div()
                                .w(160.0)
                                .h(130.0)
                                .items_center()
                                .justify_center()
                                .pt(3.0)
                                .child(
                                    div()
                                        .w(140.0)
                                        .h(110.0)
                                        .bg(Color::rgba(0.15, 0.15, 0.18, 1.0))
                                        .rounded(8.0)
                                        .items_center()
                                        .justify_center()
                                        .flex_col()
                                        .gap(2.0)
                                        .child(
                                            text("Pull down to refresh")
                                                .size(11.0)
                                                .color(Color::rgba(0.5, 0.5, 0.6, 1.0)),
                                        )
                                        .child(
                                            div()
                                                .w(120.0)
                                                .h(60.0)
                                                .bg(Color::rgba(0.2, 0.2, 0.25, 1.0))
                                                .rounded(6.0)
                                                .items_center()
                                                .justify_center()
                                                .child(
                                                    text("Content")
                                                        .size(12.0)
                                                        .color(Color::rgba(0.6, 0.6, 0.7, 1.0)),
                                                ),
                                        ),
                                ),
                        ),
                    ),
            ),
    )
}

fn list_item(label: &str) -> Div {
    div()
        .w(160.0)
        .h_fit()
        .p(4.0)
        .bg(Color::rgba(0.5, 0.8, 0.6, 1.0))
        .rounded(4.0)
        .items_center()
        .justify_center()
        .child(text(label).size(11.0).color(Color::WHITE).no_wrap())
}

/// Showcase the motion() API
fn api_showcase() -> Scroll {
    scroll()
        .w_full()
        .h(600.0)
        .direction(ScrollDirection::Vertical)
        .p(20.0)
        .rounded(12.0)
        .bg(Color::from_hex(0x222222))
        .child(
            div()
                .w_full()
                .flex_col()
                .gap(8.0)
                .child(
                    text("motion() API Reference")
                        .size(24.0)
                        .weight(FontWeight::ExtraBold)
                        .color(Color::WHITE),
                )
                .child(
                    code(
                        "// Single element with animations
motion()
    .fade_in(300)
    .scale_in(300)
    .fade_out(200)
    .child(my_element)",
                    )
                    .syntax(SyntaxConfig::new(RustHighlighter::new()))
                    .font_size(12.0)
                    .w_full(),
                )
                .child(
                    code(
                        "// Staggered list animation
motion()
    .stagger(StaggerConfig::new(50, AnimationPreset::fade_in(300))
        .from_center())
    .children(items.iter().map(|i| card(i)))",
                    )
                    .syntax(SyntaxConfig::new(RustHighlighter::new()))
                    .font_size(12.0)
                    .w_full(),
                )
                .child(
                    code(
                        "// Slide animations
motion()
    .slide_in(SlideDirection::Left, 400)
    .slide_out(SlideDirection::Right, 300)
    .child(panel)",
                    )
                    .syntax(SyntaxConfig::new(RustHighlighter::new()))
                    .font_size(12.0)
                    .w_full(),
                )
                .child(
                    code(
                        "// Custom animation with presets
motion()
    .enter_animation(AnimationPreset::bounce_in(500))
    .exit_animation(AnimationPreset::fade_out(200))
    .child(modal)",
                    )
                    .syntax(SyntaxConfig::new(RustHighlighter::new()))
                    .font_size(12.0)
                    .w_full(),
                )
                .child(
                    div()
                        .p(12.0)
                        .bg(Color::rgba(0.2, 0.15, 0.1, 1.0))
                        .rounded(6.0)
                        .child(
                            text("Note: Visual animations pending RenderTree integration")
                                .size(11.0)
                                .color(Color::rgba(1.0, 0.7, 0.4, 1.0)),
                        ),
                ),
        )
}

fn demo_card(title: &str, subtitle: &str) -> Div {
    div()
        .w(180.0)
        .flex_col()
        .gap(8.0)
        .px(4.0)
        .py(10.0)
        .bg(Color::rgba(0.14, 0.14, 0.18, 1.0))
        .rounded(12.0)
        .items_center()
        .child(
            text(title)
                .size(14.0)
                .weight(FontWeight::SemiBold)
                .color(Color::WHITE),
        )
        .child(
            text(subtitle)
                .size(10.0)
                .color(Color::rgba(0.5, 0.5, 0.6, 1.0)),
        )
}
