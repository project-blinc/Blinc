//! Motion Container Demo
//!
//! Demonstrates the motion() element for declarative enter/exit animations:
//! - Single element with fade/scale animations
//! - Staggered list animations with configurable delays
//! - Different stagger directions (forward, reverse, from center)
//! - Various animation presets (fade, scale, slide, bounce, pop)
//!
//! Note: Enter/exit animations require RenderTree integration (pending).
//! This example showcases the API design and stagger delay calculations.
//!
//! Run with: cargo run -p blinc_app --example motion_demo --features windowed

use blinc_animation::AnimationPreset;
use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_core::Color;
use blinc_layout::motion::{motion, StaggerConfig};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = WindowConfig {
        title: "Motion Container Demo".to_string(),
        width: 900,
        height: 700,
        ..Default::default()
    };

    WindowedApp::run(config, |ctx| build_ui(ctx))
}

fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.08, 0.08, 0.12, 1.0))
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
            div()
                .w_full()
                .flex_row()
                .justify_center()
                .gap(5.0)
                .flex_wrap()
                .child(single_element_demo())
                .child(stagger_forward_demo())
                .child(stagger_reverse_demo())
                .child(stagger_center_demo()),
        )
        .child(api_showcase())
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

fn list_item(label: &str) -> Div {
    div()
        .w(160.0)
        .h(24.0)
        .bg(Color::rgba(0.5, 0.8, 0.6, 1.0))
        .rounded(4.0)
        .items_center()
        .justify_center()
        .child(text(label).text_center().size(11.0).color(Color::WHITE))
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
