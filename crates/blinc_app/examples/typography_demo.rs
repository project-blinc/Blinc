//! Typography Demo
//!
//! This example demonstrates typography helpers:
//! - Headings: h1-h6, heading()
//! - Inline text: b, span, small, label, muted, p, caption, inline_code
//! - Font families: system, monospace, serif, sans_serif, custom fonts
//!
//! For table examples, see `table_demo.rs`
//!
//! Run with: cargo run -p blinc_app --example typography_demo --features windowed

use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_core::Color;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = WindowConfig {
        title: "Typography Demo".to_string(),
        width: 900,
        height: 700,
        resizable: true,
        ..Default::default()
    };

    WindowedApp::run(config, |ctx| build_ui(ctx))
}

fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.08, 0.08, 0.1, 1.0))
        .flex_col()
        .gap(8.0)
        .p(24.0)
        .child(
            scroll()
                .w_full()
                .h_full()
                .direction(ScrollDirection::Vertical)
                .child(
                    div()
                        .w_full() // Constrain width to scroll viewport for text wrapping
                        .flex_col()
                        .gap(32.0)
                        .p(8.0)
                        .child(typography_section())
                        .child(inline_text_section())
                        .child(font_family_section()),
                ),
        )
}

/// Demonstrates heading helpers h1-h6
fn typography_section() -> Div {
    div()
        .flex_col()
        .gap(12.0)
        .child(
            div()
                .flex_col()
                .gap(4.0)
                .child(h1("Typography Helpers").color(Color::WHITE))
                .child(muted("Semantic text elements with sensible defaults")),
        )
        .child(
            div()
                .bg(Color::rgba(0.12, 0.12, 0.15, 1.0))
                .rounded(8.0)
                .p(16.0)
                .flex_col()
                .gap(2.0)
                .child(h1("Heading 1 (32px, bold)").color(Color::WHITE))
                .child(h2("Heading 2 (24px, bold)").color(Color::WHITE))
                .child(h3("Heading 3 (20px, semibold)").color(Color::WHITE))
                .child(h4("Heading 4 (18px, semibold)").color(Color::WHITE))
                .child(h5("Heading 5 (16px, medium)").color(Color::WHITE))
                .child(h6("Heading 6 (14px, medium)").color(Color::WHITE)),
        )
        .child(
            div()
                .bg(Color::rgba(0.12, 0.12, 0.15, 1.0))
                .rounded(8.0)
                .p(16.0)
                .flex_col()
                .gap(8.0)
                .child(h4("Dynamic Heading Level").color(Color::WHITE))
                .child(
                    div()
                        .flex_row()
                        .gap(16.0)
                        .child(heading(1, "Level 1").color(Color::from_hex(0x66B2FF)))
                        .child(heading(3, "Level 3").color(Color::from_hex(0x66B2FF)))
                        .child(heading(5, "Level 5").color(Color::from_hex(0x66B2FF))),
                ),
        )
}

/// Demonstrates inline text helpers
fn inline_text_section() -> Div {
    div()
    .w_full()
        .flex_col()
        .gap(12.0)
        .child(h2("Inline Text Helpers").color(Color::WHITE))
        .child(
            div()
                .w_full() // Constrain width for text wrapping
                .bg(Color::rgba(0.12, 0.12, 0.15, 1.0))
                .rounded(8.0)
                .p(16.0)
                .flex_col()
                .gap(12.0)
                // Bold text
                .child(
                    div()
                        .flex_row()
                        .gap(8.0)
                        .items_center()
                        .child(label("b() / strong():").color(Color::GRAY))
                        .child(b("This text is bold").color(Color::WHITE)),
                )
                // Muted text
                .child(
                    div()
                        .flex_row()
                        .gap(8.0)
                        .items_center()
                        .child(label("muted():").color(Color::GRAY))
                        .child(muted("This is secondary/muted text")),
                )
                // Small text
                .child(
                    div()
                        .flex_row()
                        .gap(8.0)
                        .items_center()
                        .child(label("small():").color(Color::GRAY))
                        .child(small("This is small text (12px)").color(Color::WHITE)),
                )
                // Label
                .child(
                    div()
                        .flex_row()
                        .gap(8.0)
                        .items_center()
                        .child(label("label():").color(Color::GRAY))
                        .child(label("Form field label").color(Color::WHITE)),
                )
                // Caption
                .child(
                    div()
                        .flex_row()
                        .gap(8.0)
                        .items_center()
                        .child(label("caption():").color(Color::GRAY))
                        .child(caption("Figure 1: An image caption")),
                )
                // Paragraph
                .child(
                    div()
                        .flex_col()
                        .gap(4.0)
                        .w_full() // Allow text to wrap within container
                        .child(label("p():").color(Color::GRAY))
                        .child(
                            p("This is a paragraph with optimal line height (1.5) for readability. Paragraphs are styled at 16px with comfortable spacing for body text.")
                                .color(Color::WHITE),
                        ),
                )
                // Inline code
                .child(
                    div()
                        .flex_row()
                        .gap(1.0)
                        .items_center()
                        .child(label("inline_code():").color(Color::GRAY))
                        .child(span("Use ").color(Color::WHITE))
                        .child(inline_code("div().flex_col()").color(Color::GRAY))
                        .child(span(" for layouts").color(Color::WHITE)),
                ),
        )
}

/// Demonstrates font family options
fn font_family_section() -> Div {
    div()
        .w_full()
        .flex_col()
        .gap(12.0)
        .child(h2("Font Families").color(Color::WHITE))
        .child(
            div()
                .w_full()
                .bg(Color::rgba(0.12, 0.12, 0.15, 1.0))
                .rounded(8.0)
                .p(16.0)
                .flex_col()
                .gap(8.0)
                // System (default)
                .child(
                    div()
                        .flex_row()
                        .gap(8.0)
                        .items_center()
                        .child(label("System (default):").color(Color::GRAY))
                        .child(text("The quick brown fox jumps over the lazy dog").color(Color::WHITE)),
                )
                // Monospace
                .child(
                    div()
                        .flex_row()
                        .gap(8.0)
                        .items_center()
                        .child(label(".monospace():").color(Color::GRAY))
                        .child(
                            text("fn main() { println!(\"Hello\"); }")
                                .monospace()
                                .color(Color::from_hex(0x98C379)),
                        ),
                )
                // Serif
                .child(
                    div()
                        .flex_row()
                        .gap(8.0)
                        .items_center()
                        .child(label(".serif():").color(Color::GRAY))
                        .child(
                            text("The quick brown fox jumps over the lazy dog")
                                .serif()
                                .color(Color::WHITE),
                        ),
                )
                // Sans-serif
                .child(
                    div()
                        .flex_row()
                        .gap(8.0)
                        .items_center()
                        .child(label(".sans_serif():").color(Color::GRAY))
                        .child(
                            text("The quick brown fox jumps over the lazy dog")
                                .sans_serif()
                                .color(Color::WHITE),
                        ),
                )
                // Named font examples
                .child(
                    div()
                        .flex_col()
                        .gap(4.0)
                        .child(label("Named fonts with .font():").color(Color::GRAY))
                        .child(
                            div()
                                .flex_col()
                                .gap(4.0)
                                .child(
                                    text("Fira Code - fn main() { }")
                                        .font("Fira Code")
                                        .color(Color::from_hex(0xE5C07B)),
                                )
                                .child(
                                    text("Menlo - let x = 42;")
                                        .font("Menlo")
                                        .color(Color::from_hex(0x61AFEF)),
                                )
                                .child(
                                    text("SF Mono - const PI: f64 = 3.14;")
                                        .font("SF Mono")
                                        .color(Color::from_hex(0xC678DD)),
                                )
                                .child(
                                    text("Inter - Modern UI font")
                                        .font("Inter")
                                        .color(Color::WHITE),
                                ),
                        ),
                ),
        )
}
