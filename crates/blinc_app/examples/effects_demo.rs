//! Layer Effects Demo
//!
//! Showcases GPU-accelerated layer effects including:
//! - Blur (element blur, not backdrop blur)
//! - Drop shadows
//! - Glow effects
//! - Color matrix transforms (grayscale, sepia, saturation, brightness, contrast)
//!
//! Run with: cargo run -p blinc_app --example effects_demo --features windowed

use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_core::{BlurQuality, Brush, Color, Gradient, Point};
use blinc_theme::{ColorToken, ThemeState};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let config = WindowConfig {
        title: "Layer Effects Demo".to_string(),
        width: 1200,
        height: 900,
        resizable: true,
        fullscreen: false,
        ..Default::default()
    };

    WindowedApp::run(config, |ctx| build_ui(ctx))
}

fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    let theme = ThemeState::get();
    let bg = theme.color(ColorToken::Background);

    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(bg)
        .flex_col()
        .child(header())
        .child(
            scroll().w_full().h(ctx.height - 60.0).child(
                div()
                    .w_full()
                    .p(24.0)
                    .flex_col()
                    .gap(32.0)
                    .child(blur_section())
                    .child(drop_shadow_section())
                    .child(glow_section())
                    .child(color_effects_section())
                    // .child(backdrop_blur_section())
                    .child(combined_effects_section()),
            ),
        )
}

fn header() -> Div {
    let theme = ThemeState::get();

    div()
        .w_full()
        .h(60.0)
        .bg(theme.color(ColorToken::Surface))
        .flex_row()
        .items_center()
        .px(24.0)
        .child(
            text("Layer Effects Demo")
                .size(24.0)
                .weight(FontWeight::Bold)
                .color(theme.color(ColorToken::TextPrimary)),
        )
}

fn section_title(title: &str) -> Div {
    let theme = ThemeState::get();

    div().pb(12.0).child(
        text(title)
            .size(20.0)
            .weight(FontWeight::SemiBold)
            .color(theme.color(ColorToken::TextPrimary)),
    )
}

fn effect_card(label: &str) -> Div {
    let theme = ThemeState::get();

    div()
        .flex_col()
        .items_center()
        .gap(8.0)
        .child(
            div()
                .w(120.0)
                .h(120.0)
                .rounded(12.0)
                .bg(Color::from_hex(0x3b82f6)) // Blue
                .flex()
                .items_center()
                .justify_center()
                .child(
                    text("Blinc")
                        .size(20.0)
                        .weight(FontWeight::Bold)
                        .color(Color::WHITE),
                ),
        )
        .child(
            text(label)
                .size(12.0)
                .color(theme.color(ColorToken::TextSecondary)),
        )
}

fn blur_section() -> Div {
    let theme = ThemeState::get();
    let text_secondary = theme.color(ColorToken::TextSecondary);

    div()
        .w_full()
        .flex_col()
        .child(section_title("Blur Effects"))
        .child(
            div().pb(16.0).child(
                text("Element blur using Kawase blur algorithm for efficient multi-pass blur.")
                    .size(14.0)
                    .color(text_secondary),
            ),
        )
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(10.0)
                .p(10.0)
                .background(Brush::Gradient(Gradient::linear(
                    Point::new(0.0, 0.0),
                    Point::new(200.0, 200.0),
                    Color::from_hex(0xf97316),
                    Color::from_hex(0xec4899),
                )))
                // Original
                .child(effect_card("No Blur"))
                // Low blur - gradient background to show blur effect
                .child(
                    div()
                        .flex_col()
                        .items_center()
                        .gap(8.0)
                        .child(
                            div()
                                .w(120.0)
                                .h(120.0)
                                .rounded(12.0)
                                .blur(4.0)
                                .bg(Color::BLUE.with_alpha(0.3))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    text("Blinc")
                                        .size(20.0)
                                        .weight(FontWeight::Bold)
                                        .color(Color::WHITE),
                                ),
                        )
                        .child(text("blur(4)").size(12.0).color(text_secondary)),
                )
                // Medium blur
                .child(
                    div()
                        .flex_col()
                        .items_center()
                        .gap(8.0)
                        .child(
                            div()
                                .w(120.0)
                                .h(120.0)
                                .rounded(12.0)
                                .bg(Color::WHITE.with_alpha(0.3))
                                .blur(8.0)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    text("Blinc")
                                        .size(20.0)
                                        .weight(FontWeight::Bold)
                                        .color(Color::WHITE),
                                ),
                        )
                        .child(text("blur(8)").size(12.0).color(text_secondary)),
                )
                // High blur
                .child(
                    div()
                        .flex_col()
                        .items_center()
                        .gap(8.0)
                        .child(
                            div()
                                .w(120.0)
                                .h(120.0)
                                .rounded(12.0)
                                .blur(16.0)
                                .bg(Color::WHITE.with_alpha(0.3))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    text("Blinc")
                                        .size(20.0)
                                        .weight(FontWeight::Bold)
                                        .color(Color::WHITE),
                                ),
                        )
                        .child(text("blur(16)").size(12.0).color(text_secondary)),
                )
                // Very high blur with quality
                .child(
                    div()
                        .flex_col()
                        .items_center()
                        .gap(8.0)
                        .child(
                            div()
                                .w(120.0)
                                .h(120.0)
                                .rounded(12.0)
                                .blur_with_quality(24.0, BlurQuality::High)
                                .flex()
                                .bg(Color::WHITE.with_alpha(0.3))
                                .items_center()
                                .justify_center()
                                .child(
                                    text("Blinc")
                                        .size(20.0)
                                        .weight(FontWeight::Bold)
                                        .color(Color::WHITE),
                                ),
                        )
                        .child(text("blur(24, High)").size(12.0).color(text_secondary)),
                ),
        )
}

fn drop_shadow_section() -> Div {
    let theme = ThemeState::get();
    let text_secondary = theme.color(ColorToken::TextSecondary);

    div()
        .w_full()
        .flex_col()
        .child(section_title("Drop Shadows"))
        .child(
            div().pb(16.0).child(
                text("GPU-accelerated drop shadows with offset, blur, and color.")
                    .size(14.0)
                    .color(text_secondary),
            ),
        )
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(24.0)
                // No shadow
                .child(effect_card("No Shadow"))
                // Subtle shadow
                .child(
                    div()
                        .flex_col()
                        .items_center()
                        .gap(8.0)
                        .child(
                            div()
                                .w(120.0)
                                .h(120.0)
                                .rounded(12.0)
                                .bg(Color::from_hex(0x3b82f6))
                                .drop_shadow_effect(2.0, 2.0, 4.0, Color::rgba(0.0, 0.0, 0.0, 0.2))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    text("Blinc")
                                        .size(20.0)
                                        .weight(FontWeight::Bold)
                                        .color(Color::WHITE),
                                ),
                        )
                        .child(text("Subtle").size(12.0).color(text_secondary)),
                )
                // Medium shadow
                .child(
                    div()
                        .flex_col()
                        .items_center()
                        .gap(8.0)
                        .child(
                            div()
                                .w(120.0)
                                .h(120.0)
                                .rounded(12.0)
                                .bg(Color::from_hex(0x3b82f6))
                                .drop_shadow_effect(4.0, 4.0, 8.0, Color::rgba(0.0, 0.0, 0.0, 0.3))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    text("Blinc")
                                        .size(20.0)
                                        .weight(FontWeight::Bold)
                                        .color(Color::WHITE),
                                ),
                        )
                        .child(text("Medium").size(12.0).color(text_secondary)),
                )
                // Strong shadow
                .child(
                    div()
                        .flex_col()
                        .items_center()
                        .gap(8.0)
                        .child(
                            div()
                                .w(120.0)
                                .h(120.0)
                                .rounded(12.0)
                                .bg(Color::from_hex(0x3b82f6))
                                .drop_shadow_effect(6.0, 6.0, 16.0, Color::rgba(0.0, 0.0, 0.0, 0.5))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    text("Blinc")
                                        .size(20.0)
                                        .weight(FontWeight::Bold)
                                        .color(Color::WHITE),
                                ),
                        )
                        .child(text("Strong").size(12.0).color(text_secondary)),
                )
                // Colored shadow
                .child(
                    div()
                        .flex_col()
                        .items_center()
                        .gap(8.0)
                        .child(
                            div()
                                .w(120.0)
                                .h(120.0)
                                .rounded(12.0)
                                .bg(Color::from_hex(0x3b82f6))
                                .drop_shadow_effect(
                                    4.0,
                                    4.0,
                                    12.0,
                                    Color::rgba(0.23, 0.51, 0.96, 0.5),
                                )
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    text("Blinc")
                                        .size(20.0)
                                        .weight(FontWeight::Bold)
                                        .color(Color::WHITE),
                                ),
                        )
                        .child(text("Colored").size(12.0).color(text_secondary)),
                ),
        )
}

fn glow_section() -> Div {
    let theme = ThemeState::get();
    let text_secondary = theme.color(ColorToken::TextSecondary);

    div()
        .w_full()
        .flex_col()
        .child(section_title("Glow Effects"))
        .child(
            div().pb(16.0).child(
                text("Outer glow effect using drop shadow with configurable intensity.")
                    .size(14.0)
                    .color(text_secondary),
            ),
        )
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(24.0)
                // No glow
                .child(effect_card("No Glow"))
                // Subtle glow (blur=8, range=0, opacity=0.5)
                .child(
                    div()
                        .flex_col()
                        .items_center()
                        .gap(8.0)
                        .child(
                            div()
                                .w(120.0)
                                .h(120.0)
                                .rounded(12.0)
                                .bg(Color::from_hex(0x3b82f6))
                                .glow_effect(Color::from_hex(0x3b82f6), 8.0, 0.0, 0.5)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    text("Blinc")
                                        .size(20.0)
                                        .weight(FontWeight::Bold)
                                        .color(Color::WHITE),
                                ),
                        )
                        .child(text("Subtle").size(12.0).color(text_secondary)),
                )
                // Medium glow (blur=16, range=0, opacity=0.7)
                .child(
                    div()
                        .flex_col()
                        .items_center()
                        .gap(8.0)
                        .child(
                            div()
                                .w(120.0)
                                .h(120.0)
                                .rounded(12.0)
                                .bg(Color::from_hex(0x3b82f6))
                                .glow_effect(Color::from_hex(0x3b82f6), 16.0, 0.0, 0.7)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    text("Blinc")
                                        .size(20.0)
                                        .weight(FontWeight::Bold)
                                        .color(Color::WHITE),
                                ),
                        )
                        .child(text("Medium").size(12.0).color(text_secondary)),
                )
                // Intense glow (blur=24, range=0, opacity=1.0)
                .child(
                    div()
                        .flex_col()
                        .items_center()
                        .gap(8.0)
                        .child(
                            div()
                                .w(120.0)
                                .h(120.0)
                                .rounded(12.0)
                                .bg(Color::from_hex(0x3b82f6))
                                .glow_effect(Color::from_hex(0x3b82f6), 24.0, 0.0, 1.0)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    text("Blinc")
                                        .size(20.0)
                                        .weight(FontWeight::Bold)
                                        .color(Color::WHITE),
                                ),
                        )
                        .child(text("Intense").size(12.0).color(text_secondary)),
                )
                // Different color glow (pink)
                .child(
                    div()
                        .flex_col()
                        .items_center()
                        .gap(8.0)
                        .child(
                            div()
                                .w(120.0)
                                .h(120.0)
                                .rounded(12.0)
                                .bg(Color::from_hex(0x3b82f6))
                                .glow_effect(Color::from_hex(0xf43f5e), 16.0, 0.0, 0.8)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    text("Blinc")
                                        .size(20.0)
                                        .weight(FontWeight::Bold)
                                        .color(Color::WHITE),
                                ),
                        )
                        .child(text("Pink Glow").size(12.0).color(text_secondary)),
                )
                // Extended range glow (blur=16, range=12, opacity=1.0)
                .child(
                    div()
                        .flex_col()
                        .items_center()
                        .gap(8.0)
                        .child(
                            div()
                                .w(120.0)
                                .h(120.0)
                                .rounded(12.0)
                                .bg(Color::from_hex(0x3b82f6))
                                .glow_effect(Color::from_hex(0x10b981), 16.0, 12.0, 1.0)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    text("Blinc")
                                        .size(20.0)
                                        .weight(FontWeight::Bold)
                                        .color(Color::WHITE),
                                ),
                        )
                        .child(text("Extended Range").size(12.0).color(text_secondary)),
                ),
        )
}

fn color_effects_section() -> Div {
    let theme = ThemeState::get();
    let text_secondary = theme.color(ColorToken::TextSecondary);

    div()
        .w_full()
        .flex_col()
        .child(section_title("Color Matrix Effects"))
        .child(
            div().pb(16.0).child(
                text("4x5 color matrix transformations for color manipulation.")
                    .size(14.0)
                    .color(text_secondary),
            ),
        )
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(24.0)
                // Original
                .child(effect_card("Original"))
                // Grayscale
                .child(
                    div()
                        .flex_col()
                        .items_center()
                        .gap(8.0)
                        .child(
                            div()
                                .w(120.0)
                                .h(120.0)
                                .rounded(12.0)
                                .bg(Color::from_hex(0x3b82f6))
                                .grayscale()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    text("Blinc")
                                        .size(20.0)
                                        .weight(FontWeight::Bold)
                                        .color(Color::WHITE),
                                ),
                        )
                        .child(text("Grayscale").size(12.0).color(text_secondary)),
                )
                // Sepia
                .child(
                    div()
                        .flex_col()
                        .items_center()
                        .gap(8.0)
                        .child(
                            div()
                                .w(120.0)
                                .h(120.0)
                                .rounded(12.0)
                                .bg(Color::from_hex(0x3b82f6))
                                .sepia()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    text("Blinc")
                                        .size(20.0)
                                        .weight(FontWeight::Bold)
                                        .color(Color::WHITE),
                                ),
                        )
                        .child(text("Sepia").size(12.0).color(text_secondary)),
                )
                // High saturation
                .child(
                    div()
                        .flex_col()
                        .items_center()
                        .gap(8.0)
                        .child(
                            div()
                                .w(120.0)
                                .h(120.0)
                                .rounded(12.0)
                                .bg(Color::from_hex(0x3b82f6))
                                .saturation(2.0)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    text("Blinc")
                                        .size(20.0)
                                        .weight(FontWeight::Bold)
                                        .color(Color::WHITE),
                                ),
                        )
                        .child(text("Saturate 2x").size(12.0).color(text_secondary)),
                )
                // Low saturation
                .child(
                    div()
                        .flex_col()
                        .items_center()
                        .gap(8.0)
                        .child(
                            div()
                                .w(120.0)
                                .h(120.0)
                                .rounded(12.0)
                                .bg(Color::from_hex(0x3b82f6))
                                .saturation(0.5)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    text("Blinc")
                                        .size(20.0)
                                        .weight(FontWeight::Bold)
                                        .color(Color::WHITE),
                                ),
                        )
                        .child(text("Saturate 0.5x").size(12.0).color(text_secondary)),
                )
                // Brightness
                .child(
                    div()
                        .flex_col()
                        .items_center()
                        .gap(8.0)
                        .child(
                            div()
                                .w(120.0)
                                .h(120.0)
                                .rounded(12.0)
                                .bg(Color::from_hex(0x3b82f6))
                                .brightness(1.3)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    text("Blinc")
                                        .size(20.0)
                                        .weight(FontWeight::Bold)
                                        .color(Color::WHITE),
                                ),
                        )
                        .child(text("Bright 1.3x").size(12.0).color(text_secondary)),
                )
                // Contrast
                .child(
                    div()
                        .flex_col()
                        .items_center()
                        .gap(8.0)
                        .child(
                            div()
                                .w(120.0)
                                .h(120.0)
                                .rounded(12.0)
                                .bg(Color::from_hex(0x3b82f6))
                                .contrast(1.5)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    text("Blinc")
                                        .size(20.0)
                                        .weight(FontWeight::Bold)
                                        .color(Color::WHITE),
                                ),
                        )
                        .child(text("Contrast 1.5x").size(12.0).color(text_secondary)),
                ),
        )
}

fn backdrop_blur_section() -> Div {
    let theme = ThemeState::get();
    let text_secondary = theme.color(ColorToken::TextSecondary);

    div()
        .w_full()
        .flex_col()
        .child(section_title("Backdrop Blur (Glass Effect)"))
        .child(
            div().pb(16.0).child(
                text("Frosted glass effect that blurs content behind the element.")
                    .size(14.0)
                    .color(text_secondary),
            ),
        )
        .child(
            div()
                .flex_row()
                .gap(24.0)
                // Container with gradient background
                .child(
                    div()
                        .w(400.0)
                        .h(200.0)
                        .rounded(16.0)
                        .background(Brush::Gradient(Gradient::linear(
                            Point::new(0.0, 0.0),
                            Point::new(400.0, 200.0),
                            Color::from_hex(0xf43f5e),
                            Color::from_hex(0x3b82f6),
                        )))
                        .relative()
                        .child(
                            // Some content behind
                            div().absolute().top(40.0).left(40.0).child(
                                text("Content Behind Glass")
                                    .size(24.0)
                                    .weight(FontWeight::Bold)
                                    .color(Color::WHITE),
                            ),
                        )
                        .child(
                            // Glass panel
                            div()
                                .absolute()
                                .bottom(20.0)
                                .left(20.0)
                                .right(20.0)
                                .h(80.0)
                                .rounded(12.0)
                                .backdrop_blur(12.0)
                                .border(1.0, Color::rgba(1.0, 1.0, 1.0, 0.2))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    text("Frosted Glass Panel")
                                        .size(16.0)
                                        .weight(FontWeight::SemiBold)
                                        .color(Color::WHITE),
                                ),
                        ),
                )
                // Another example with radial gradient
                .child(
                    div()
                        .w(300.0)
                        .h(200.0)
                        .rounded(16.0)
                        .background(Brush::Gradient(Gradient::radial(
                            Point::new(150.0, 100.0),
                            150.0,
                            Color::from_hex(0x10b981),
                            Color::from_hex(0x0f172a),
                        )))
                        .relative()
                        .child(
                            div().absolute().top(30.0).left(30.0).child(
                                text("Radial BG")
                                    .size(20.0)
                                    .weight(FontWeight::Bold)
                                    .color(Color::WHITE),
                            ),
                        )
                        .child(
                            div()
                                .absolute()
                                .bottom(20.0)
                                .left(20.0)
                                .w(120.0)
                                .h(60.0)
                                .rounded(8.0)
                                .backdrop_blur_light()
                                .border(1.0, Color::rgba(1.0, 1.0, 1.0, 0.15))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(text("Light").size(14.0).color(Color::WHITE)),
                        )
                        .child(
                            div()
                                .absolute()
                                .bottom(20.0)
                                .right(20.0)
                                .w(120.0)
                                .h(60.0)
                                .rounded(8.0)
                                .backdrop_blur_heavy()
                                .border(1.0, Color::rgba(1.0, 1.0, 1.0, 0.15))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(text("Heavy").size(14.0).color(Color::WHITE)),
                        ),
                ),
        )
}

fn combined_effects_section() -> Div {
    let theme = ThemeState::get();
    let text_secondary = theme.color(ColorToken::TextSecondary);

    div()
        .w_full()
        .flex_col()
        .child(section_title("Combined Effects"))
        .child(
            div().pb(16.0).child(
                text("Multiple effects can be chained together.")
                    .size(14.0)
                    .color(text_secondary),
            ),
        )
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(24.0)
                // Blur + Shadow
                .child(
                    div()
                        .flex_col()
                        .items_center()
                        .gap(8.0)
                        .child(
                            div()
                                .w(120.0)
                                .h(120.0)
                                .rounded(12.0)
                                .bg(Color::from_hex(0x8b5cf6))
                                .blur(4.0)
                                .drop_shadow_effect(4.0, 4.0, 8.0, Color::rgba(0.0, 0.0, 0.0, 0.4))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    text("Blinc")
                                        .size(20.0)
                                        .weight(FontWeight::Bold)
                                        .color(Color::WHITE),
                                ),
                        )
                        .child(text("Blur + Shadow").size(12.0).color(text_secondary)),
                )
                // Grayscale + Blur
                .child(
                    div()
                        .flex_col()
                        .items_center()
                        .gap(8.0)
                        .child(
                            div()
                                .w(120.0)
                                .h(120.0)
                                .rounded(12.0)
                                .bg(Color::from_hex(0xf97316))
                                .grayscale()
                                .blur(6.0)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    text("Blinc")
                                        .size(20.0)
                                        .weight(FontWeight::Bold)
                                        .color(Color::WHITE),
                                ),
                        )
                        .child(text("Gray + Blur").size(12.0).color(text_secondary)),
                )
                // Sepia + Glow
                .child(
                    div()
                        .flex_col()
                        .items_center()
                        .gap(8.0)
                        .child(
                            div()
                                .w(120.0)
                                .h(120.0)
                                .rounded(12.0)
                                .bg(Color::from_hex(0x10b981))
                                .sepia()
                                .glow_effect(Color::from_hex(0xa3852f),4.0, 5.0,  0.6)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    text("Blinc")
                                        .size(20.0)
                                        .weight(FontWeight::Bold)
                                        .color(Color::WHITE),
                                ),
                        )
                        .child(text("Sepia + Glow").size(12.0).color(text_secondary)),
                )
                // Saturation + Contrast + Shadow
                .child(
                    div()
                        .flex_col()
                        .items_center()
                        .gap(8.0)
                        .child(
                            div()
                                .w(120.0)
                                .h(120.0)
                                .rounded(12.0)
                                .bg(Color::from_hex(0xec4899))
                                .saturation(1.5)
                                .contrast(1.2)
                                .drop_shadow_effect(
                                    3.0,
                                    3.0,
                                    10.0,
                                    Color::rgba(0.93, 0.28, 0.6, 0.5),
                                )
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    text("Blinc")
                                        .size(20.0)
                                        .weight(FontWeight::Bold)
                                        .color(Color::WHITE),
                                ),
                        )
                        .child(text("Sat+Con+Sha").size(12.0).color(text_secondary)),
                ),
        )
}
