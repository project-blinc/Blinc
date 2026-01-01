//! Rich Text Element Demo
//!
//! This example demonstrates the rich_text element for inline text formatting
//! with HTML-like tags.
//!
//! Features demonstrated:
//! - HTML-like inline formatting (<b>, <i>, <u>, <s>)
//! - Nested tags
//! - Inline colors with <span color="...">
//! - Links with <a href="...">
//! - Range-based programmatic styling API
//! - Entity decoding (&lt;, &gt;, &amp;, etc.)
//!
//! Run with: cargo run -p blinc_app --example rich_text_demo --features windowed

use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_core::Color;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = WindowConfig {
        title: "Rich Text Demo".to_string(),
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
        .p(16.0)
        .items_center()
        // Title
        .child(text("Rich Text Demo").size(28.0).color(Color::WHITE))
        .child(
            text("Inline formatting with HTML-like tags")
                .size(16.0)
                .color(Color::rgba(0.6, 0.6, 0.7, 1.0)),
        )
        // Demo sections in a scrollable area
        .child(
            scroll()
                .rounded(8.0)
                .w_full()
                .flex_grow()
                .direction(ScrollDirection::Vertical)
                .bg(Color::from_hex(0x1a1a1a))
                .child(
                    div()
                        .w_full()
                        .p(16.0)
                        .flex_col()
                        .gap(24.0)
                        .child(basic_formatting_section())
                        .child(nested_tags_section())
                        .child(colors_section())
                        .child(links_section())
                        .child(entities_section())
                        .child(range_api_section()),
                ),
        )
}

/// Basic formatting: bold, italic, underline, strikethrough
fn basic_formatting_section() -> Div {
    section(
        "Basic Formatting",
        div()
            .flex_col()
            .gap(12.0)
            .child(
                rich_text("This text has <b>bold</b> words in it.")
                    .size(16.0)
                    .default_color(Color::WHITE),
            )
            .child(
                rich_text("This text has <i>italic</i> words in it.")
                    .size(16.0)
                    .default_color(Color::WHITE),
            )
            .child(
                rich_text("This text has <u>underlined</u> words in it.")
                    .size(16.0)
                    .default_color(Color::WHITE),
            )
            .child(
                rich_text("This text has <s>strikethrough</s> words in it.")
                    .size(16.0)
                    .default_color(Color::WHITE),
            )
            .child(
                rich_text("Using <strong>strong</strong> and <em>em</em> tags also works.")
                    .size(16.0)
                    .default_color(Color::WHITE),
            ),
    )
}

/// Nested tags demonstration
fn nested_tags_section() -> Div {
    section(
        "Nested Tags",
        div()
            .flex_col()
            .gap(12.0)
            .child(
                rich_text("This is <b>bold with <i>italic inside</i></b>.")
                    .size(16.0)
                    .default_color(Color::WHITE),
            )
            .child(
                rich_text(
                    "Mix styles: <b><u>bold underlined</u></b> and <i><s>italic struck</s></i>.",
                )
                .size(16.0)
                .default_color(Color::WHITE),
            )
            .child(
                rich_text("<b><i><u>All three styles combined!</u></i></b>")
                    .size(18.0)
                    .default_color(Color::WHITE),
            ),
    )
}

/// Inline colors with span
fn colors_section() -> Div {
    section(
        "Inline Colors",
        div()
            .flex_col()
            .gap(12.0)
            .child(
                rich_text("Colors: <span color=\"#FF0000\">red</span>, <span color=\"#00FF00\">green</span>, <span color=\"#0000FF\">blue</span>.")
                    .size(16.0)
                    .default_color(Color::WHITE),
            )
            .child(
                rich_text("Named colors: <span color=\"orange\">orange</span>, <span color=\"purple\">purple</span>, <span color=\"cyan\">cyan</span>.")
                    .size(16.0)
                    .default_color(Color::WHITE),
            )
            .child(
                rich_text("<span color=\"gold\"><b>Warning:</b></span> This is an important message!")
                    .size(16.0)
                    .default_color(Color::WHITE),
            )
            .child(
                rich_text("<span color=\"#FF4444\">Error:</span> Something went <span color=\"crimson\"><b>wrong</b></span>.")
                    .size(16.0)
                    .default_color(Color::WHITE),
            ),
    )
}

/// Links with anchor tags
fn links_section() -> Div {
    section(
        "Links",
        div()
            .flex_col()
            .gap(12.0)
            .child(
                rich_text("Visit <a href=\"https://example.com\">our website</a> for more info.")
                    .size(16.0)
                    .default_color(Color::WHITE),
            )
            .child(
                rich_text("Check the <a href=\"https://docs.example.com\">documentation</a> or <a href=\"https://github.com\">source code</a>.")
                    .size(16.0)
                    .default_color(Color::WHITE),
            )
            .child(
                rich_text("Links with formatting: <a href=\"#\"><b>bold link</b></a> and <a href=\"#\"><i>italic link</i></a>.")
                    .size(16.0)
                    .default_color(Color::WHITE),
            ),
    )
}

/// HTML entity decoding
fn entities_section() -> Div {
    section(
        "Entity Decoding",
        div()
            .flex_col()
            .gap(12.0)
            .child(
                rich_text("Escaped characters: &lt;div&gt; &amp; &quot;quotes&quot;")
                    .size(16.0)
                    .default_color(Color::WHITE),
            )
            .child(
                rich_text("Typography: &ldquo;Smart quotes&rdquo; &mdash; and &hellip;")
                    .size(16.0)
                    .default_color(Color::WHITE),
            )
            .child(
                rich_text("Symbols: &copy; 2024 &bull; All Rights Reserved &trade;")
                    .size(16.0)
                    .default_color(Color::WHITE),
            )
            .child(
                rich_text("Numeric: &#65;&#66;&#67; and hex &#x41;&#x42;&#x43;")
                    .size(16.0)
                    .default_color(Color::WHITE),
            ),
    )
}

/// Range-based API for programmatic styling
fn range_api_section() -> Div {
    section(
        "Range-Based API",
        div()
            .flex_col()
            .gap(12.0)
            .child(
                div()
                    .flex_col()
                    .gap(4.0)
                    .child(
                        text("Programmatic styling using byte ranges:")
                            .size(14.0)
                            .color(Color::rgba(0.7, 0.7, 0.7, 1.0)),
                    )
                    .child(
                        // "Hello World" with programmatic styling
                        rich_text("Hello World")
                            .bold_range(0..5) // "Hello" is bold
                            .color_range(6..11, Color::CYAN) // "World" is cyan
                            .size(18.0)
                            .default_color(Color::WHITE),
                    ),
            )
            .child(
                div()
                    .flex_col()
                    .gap(4.0)
                    .child(
                        text("Multiple styles on the same text:")
                            .size(14.0)
                            .color(Color::rgba(0.7, 0.7, 0.7, 1.0)),
                    )
                    .child(
                        rich_text("Important Notice: Please read carefully!")
                            .bold_range(0..16) // "Important Notice" bold
                            .color_range(0..9, Color::ORANGE) // "Important" orange
                            .underline_range(18..39) // "Please read carefully" underlined
                            .size(16.0)
                            .default_color(Color::WHITE),
                    ),
            ),
    )
}

/// Helper to create a section with a title and content
fn section(title: &str, content: Div) -> Div {
    div()
        .flex_col()
        .gap(8.0)
        .p(12.0)
        .rounded(8.0)
        .bg(Color::rgba(0.12, 0.12, 0.15, 1.0))
        .child(
            text(title)
                .size(20.0)
                .color(Color::rgba(0.9, 0.9, 1.0, 1.0))
                .bold(),
        )
        .child(div().h(1.0).w_full().bg(Color::rgba(0.3, 0.3, 0.4, 0.5)))
        .child(content)
}
